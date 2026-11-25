import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import NovaCrypto
import SubstrateSdk
import Testing
@testable import Coinage

@Suite("Transfer status re-evaluates on finalized heads", .timeLimit(.minutes(1)))
struct CoinageTransferStatusServiceTests {
    private let chain = CoinageFakeChain(initialState: .empty)
    private let chainFactory: FakePinnedChainViewFactory<CoinageChainState>
    private let database = StubDatabaseDependencyFactory()
    private let coinQuery = StubCoinOnChainQuery()
    private let store = MockCoinageTxRepository()
    private let group: CoinageTxGroupId = "chat-reclaim:message-1"
    private let service: CoinageTransferStatusService
    private let secret: Data
    private let publicKey: PublicKey

    init() throws {
        chain.produceBlock()
        chain.produceBlock()
        chain.finalize(upTo: 1)
        chainFactory = FakePinnedChainViewFactory(chain: chain)

        let keypair = try SNKeyFactory().createKeypair(fromSeed: Data(repeating: 0xAB, count: 32))
        secret = keypair.privateKey().rawData()
        publicKey = keypair.publicKey().rawData()

        service = CoinageTransferStatusService(
            databaseFactory: database,
            chainViewFactory: chainFactory,
            chainId: "status-chain",
            coinOnChainQuery: coinQuery,
            txService: MockCoinageTxService(store: store),
            snKeyFactory: SNKeyFactory(),
            logger: nil
        )
    }

    @Test("a new finalized head turns a best-head claim into a finalized one without a new snapshot")
    func finalizedHeadCompletesTheClaim() async throws {
        coinQuery.setPresent(publicKey, atBlockHash: chain.finalizedHead.hash)
        database.trackedCoins.send([claimedOnBestHead()])

        var iterator = service.subscribeStatuses(coinKeys: [secret], reclaimGroupId: group).makeAsyncIterator()
        let beforeFinality = try await iterator.next()
        #expect(beforeFinality?[publicKey]?.status == .claimed(finalized: false))

        chain.finalize(upTo: 2)
        chainFactory.emitFinalizedHead(2)

        let afterFinality = try await iterator.next()
        #expect(afterFinality?[publicKey]?.status == .claimed(finalized: true))
    }

    @Test("a finalized head that changes nothing emits nothing")
    func unchangedHeadIsSilent() async throws {
        coinQuery.setPresent(publicKey, atBlockHash: chain.finalizedHead.hash)
        database.trackedCoins.send([claimedOnBestHead()])

        var iterator = service.subscribeStatuses(coinKeys: [secret], reclaimGroupId: group).makeAsyncIterator()
        _ = try await iterator.next()

        chainFactory.emitFinalizedHead(1)
        chain.finalize(upTo: 2)
        chainFactory.emitFinalizedHead(2)

        let next = try await iterator.next()
        #expect(next?[publicKey]?.status == .claimed(finalized: true))
    }

    @Test("no coins to watch is a finished stream, not one waiting for a head")
    func emptyKeySetFinishes() async throws {
        var emissions = 0
        for try await _ in service.subscribeStatuses(coinKeys: [], reclaimGroupId: group) {
            emissions += 1
        }
        #expect(emissions == 0)
    }

    @Test("a failed reclaim reads reclaimFailed until a later attempt for its coin supersedes it")
    func failedReclaimIsSupersededByRetry() async throws {
        coinQuery.setPresent(publicKey, atBlockHash: chain.finalizedHead.hash)
        database.trackedCoins.send([awaitingOnChain()])
        try await store.markHandedOff(.coin(0, publicKey))
        try await store.register(reclaimEntry(status: .failure))

        var iterator = service.subscribeStatuses(coinKeys: [secret], reclaimGroupId: group).makeAsyncIterator()
        let failed = try await iterator.next()
        #expect(failed?[publicKey]?.status == .reclaimFailed)

        try await store.register(reclaimEntry(status: .pending))

        let retried = try await iterator.next()
        #expect(retried?[publicKey]?.status == .awaitingClaim(finalized: true))
    }

    @Test("a coin under reclaim is left out of the finalized presence read")
    func reclaimTrackedCoinSkipsPresenceRead() async throws {
        database.trackedCoins.send([claimedOnBestHead(consumer: .pendingSuccess)])

        var iterator = service.subscribeStatuses(coinKeys: [secret], reclaimGroupId: group).makeAsyncIterator()
        let first = try await iterator.next()

        #expect(first?[publicKey]?.status == .reclaimed(finalized: false))
        #expect(coinQuery.reads.isEmpty)
    }
}

private extension CoinageTransferStatusServiceTests {
    func claimedOnBestHead(consumer: CoinageTxStatus? = nil) -> TrackedCoin {
        trackedCoin(isOnchain: false, consumer: consumer)
    }

    func awaitingOnChain() -> TrackedCoin {
        trackedCoin(isOnchain: true, consumer: nil)
    }

    func trackedCoin(isOnchain: Bool, consumer: CoinageTxStatus?) -> TrackedCoin {
        TrackedCoin(
            coin: Coin(
                exponent: 1,
                derivationIndex: 0,
                age: 3,
                isOnchain: isOnchain,
                handoffMark: .committed,
                publicKey: publicKey
            ),
            state: CoinageAssetState(handedOff: true, consumerStatus: consumer, minterStatus: .finalizedSuccess)
        )
    }

    func reclaimEntry(status: CoinageTxStatus) -> CoinageTxEntry {
        CoinageTxEntry(
            inputs: [.coin(.reclaimed(0, publicKey))],
            outputs: [],
            groupId: group,
            txHash: Data(repeating: 0xAB, count: 32),
            checkpoint: BlockRef(number: 0, hash: Data(repeating: 0, count: 32)),
            mortality: 300,
            status: status
        )
    }
}
