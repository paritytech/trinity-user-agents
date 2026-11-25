import AsyncExtensions
import Foundation
import FoundationExt
import FoundationExtTestSupport
import NovaCrypto
import SubstrateSdk
import Testing
@testable import Coinage

@Suite("Reclaim Coins Service")
struct ReclaimCoinsServiceTests {
    private let now = Date(timeIntervalSince1970: 1_700_000_000)
    private let group: CoinageTxGroupId = "message-1"
    private let snKeyFactory = SNKeyFactory()

    @Test("only committed, on-chain and unconsumed coins are reclaimed")
    func reclaimsOnlyEligibleCoins() async throws {
        let eligible = try makeCoin(seed: 1, index: 1, handoff: .committed, onChain: true)
        let notOnChain = try makeCoin(seed: 2, index: 2, handoff: .committed, onChain: false)
        let consumed = try makeCoin(seed: 3, index: 3, handoff: .committed, onChain: true, consumer: .pendingSuccess)
        let neverHandedOff = try makeCoin(seed: 4, index: 4, handoff: .none, onChain: true)
        let all = [eligible, notOnChain, consumed, neverHandedOff]
        let harness = makeHarness(tracked: all)

        try await harness.sut.reclaim(coinKeys: all.map(\.secret), groupId: group)

        let requests = harness.txService.scheduledRequests
        #expect(requests.count == 1)
        #expect(requests.first?.inputs == [.coin(.reclaimed(1, eligible.publicKey))])
    }

    @Test("each destination keeps its coin's exponent and chain, plus one transfer hop of the bundle size")
    func destinationMintsLikeItsCoin() async throws {
        let parentHops: [Hop] = [.split(fanout: 2)]
        let first = try makeCoin(seed: 1, index: 1, handoff: .committed, onChain: true, exponent: 5, hops: parentHops)
        let second = try makeCoin(seed: 2, index: 2, handoff: .committed, onChain: true, exponent: 3, hops: parentHops)
        let harness = makeHarness(tracked: [first, second])

        try await harness.sut.reclaim(coinKeys: [first.secret, second.secret], groupId: group)

        let minted = await harness.allocator.mintedCoins
        #expect(minted.map(\.exponent) == [5, 3])
        #expect(minted.allSatisfy { $0.hops == [.split(fanout: 2), .transfer(bundleSize: 2)] })
        #expect(minted.allSatisfy { $0.recyclerFungibility == first.coin.recyclerFungibility })
        #expect(harness.txService.scheduledRequests.flatMap(\.outputs) == minted.map {
            OwnAsset.coin($0.derivationIndex, $0.publicKey)
        })
    }

    @Test("the reclaim is scheduled under the reclaim policy and group, retried until its window closes")
    func scheduledUnderReclaimPolicyAndGroup() async throws {
        let coin = try makeCoin(seed: 1, index: 1, handoff: .committed, onChain: true)
        let harness = makeHarness(tracked: [coin])

        try await harness.sut.reclaim(coinKeys: [coin.secret], groupId: group)

        let policy = try #require(harness.txService.scheduledRequests.first?.policy)
        #expect(policy.id == CoinageSubmissionParams.reclaimPolicyId)
        let params = try CoinageSubmissionParams.decodeTransfer(policy.params)
        #expect(params.buildUntil == now.addingTimeInterval(CoinageConstants.reclaimRetryWindow))
        #expect(params.retryFailures)
        #expect(harness.txService.scheduledGroupIds == [group])
    }

    @Test("nothing reclaimable schedules nothing")
    func nothingEligibleSchedulesNothing() async throws {
        let coin = try makeCoin(seed: 1, index: 1, handoff: .committed, onChain: false)
        let harness = makeHarness(tracked: [coin])

        try await harness.sut.reclaim(coinKeys: [coin.secret], groupId: group)

        #expect(harness.txService.scheduledRequests.isEmpty)
        #expect(await harness.allocator.mintedCoins.isEmpty)
    }

    @Test("a reclaim already being scheduled for the group is not scheduled twice, a later one is")
    func inFlightReclaimIsNotRepeated() async throws {
        let coin = try makeCoin(seed: 1, index: 1, handoff: .committed, onChain: true)
        let gate = GatedDateProvider(now)
        let harness = makeHarness(tracked: [coin], dateProvider: gate)

        let first = Task { try await harness.sut.reclaim(coinKeys: [coin.secret], groupId: group) }
        await gate.waitForReader()
        try await harness.sut.reclaim(coinKeys: [coin.secret], groupId: group)
        #expect(await harness.allocator.mintedCoins.isEmpty)

        gate.open()
        try await first.value
        #expect(harness.txService.scheduledRequests.count == 1)

        try await harness.sut.reclaim(coinKeys: [coin.secret], groupId: group)
        #expect(harness.txService.scheduledRequests.count == 2)
    }
}

private extension ReclaimCoinsServiceTests {
    struct FakeCoin {
        let secret: Data
        let publicKey: PublicKey
        let tracked: TrackedCoin

        var coin: Coin { tracked.coin }
    }

    struct Harness {
        let sut: ReclaimCoinsService
        let txService: MockCoinageTxService
        let allocator: MockCoinAllocator
    }

    func makeCoin(
        seed: UInt8,
        index: CoinageKeyIndex,
        handoff: CoinHandoffMark,
        onChain: Bool,
        exponent: Int16 = 3,
        hops: [Hop] = [],
        consumer: CoinageTxStatus? = nil
    ) throws -> FakeCoin {
        let secret = try snKeyFactory.createKeypair(fromSeed: Data(repeating: seed, count: 32)).privateKey().rawData()
        let publicKey = try snKeyFactory.createPublicKey(fromSecret: secret).rawData()
        let coin = Coin(
            exponent: exponent,
            derivationIndex: index,
            age: 1,
            isOnchain: onChain,
            handoffMark: handoff,
            recyclerFungibility: 80,
            hops: hops,
            publicKey: publicKey
        )
        let state = CoinageAssetState(
            handedOff: handoff == .committed,
            consumerStatus: consumer,
            minterStatus: .finalizedSuccess
        )
        return FakeCoin(secret: secret, publicKey: publicKey, tracked: TrackedCoin(coin: coin, state: state))
    }

    func makeHarness(
        tracked: [FakeCoin],
        store: MockCoinageTxRepository = MockCoinageTxRepository(),
        dateProvider: (any DateProviding)? = nil
    ) -> Harness {
        let databaseFactory = StubDatabaseDependencyFactory()
        databaseFactory.trackedCoins.send(tracked.map(\.tracked))

        let txService = MockCoinageTxService(store: store)
        let allocator = MockCoinAllocator()
        let sut = ReclaimCoinsService(
            txService: txService,
            databaseFactory: databaseFactory,
            minter: allocator,
            snKeyFactory: snKeyFactory,
            dateProvider: dateProvider ?? FixedDateProvider(now),
            logger: nil
        )

        return Harness(sut: sut, txService: txService, allocator: allocator)
    }
}

/// Holds every reader until `open()`, so a reclaim can be caught between its coin fetch and its mint.
private final class GatedDateProvider: DateProviding, @unchecked Sendable {
    private let date: Date
    private let isOpen = AsyncCurrentValueSubject<Bool>(false)
    private let readers = AsyncCurrentValueSubject<Int>(0)

    init(_ date: Date) {
        self.date = date
    }

    func read() async -> Date {
        readers.send(readers.value + 1)
        for await open in isOpen where open {
            break
        }
        return date
    }

    func open() {
        isOpen.send(true)
    }

    func waitForReader() async {
        for await count in readers where count > 0 {
            break
        }
    }
}
