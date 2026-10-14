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
            snKeyFactory: SNKeyFactory(),
            logger: nil
        )
    }

    @Test("a new finalized head turns a best-head claim into a finalized one without a new snapshot")
    func finalizedHeadCompletesTheClaim() async throws {
        coinQuery.setPresent(publicKey, atBlockHash: chain.finalizedHead.hash)
        database.trackedCoins.send([claimedOnBestHead()])

        var iterator = service.subscribeStatuses(coinKeys: [secret]).makeAsyncIterator()
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

        var iterator = service.subscribeStatuses(coinKeys: [secret]).makeAsyncIterator()
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
        for try await _ in service.subscribeStatuses(coinKeys: []) {
            emissions += 1
        }
        #expect(emissions == 0)
    }
}

private extension CoinageTransferStatusServiceTests {
    func claimedOnBestHead() -> TrackedCoin {
        TrackedCoin(
            coin: Coin(
                exponent: 1,
                derivationIndex: 0,
                age: 3,
                isOnchain: false,
                handoffMark: .committed,
                publicKey: publicKey
            ),
            state: CoinageAssetState(handedOff: true, consumerStatus: nil, minterStatus: .finalizedSuccess)
        )
    }
}
