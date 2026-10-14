import DurableTransactions
import Foundation
import Testing
@testable import Coinage

/// A split rebuilt from its ledger row must mint exactly the coins whose keys the recipient already
/// holds — that is the whole reason the row is kept rather than the payment planned again. So a row that
/// cannot be resolved in full resolves to nothing at all.
@Suite("Split Rebuild")
struct SplitRebuildTests {
    private let policyId = CoinageSubmissionParams.splitPolicyId

    @Test("terms come from the transfer params")
    func termsAreDecoded() throws {
        let rebuild = makeRebuild(coins: [])

        let terms = try #require(rebuild.terms(of: RebuildFixtures.transferParams(inSeconds: 60, retryFailures: false)))

        #expect(terms.deadline == RebuildFixtures.now.addingTimeInterval(60))
        #expect(!terms.retriesFailures)
    }

    @Test("params that cannot be read have no terms")
    func unreadableParamsHaveNoTerms() {
        let rebuild = makeRebuild(coins: [])

        #expect(rebuild.terms(of: Data([0xFF])) == nil)
    }

    @Test("the recorded input and outputs are resolved into a split")
    func resolvesRecordedSplit() async throws {
        let input = RebuildFixtures.coin(1)
        let outputs = [RebuildFixtures.coin(2), RebuildFixtures.coin(3)]
        let rebuild = makeRebuild(coins: [input] + outputs)
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [.coin(.own(1, input.publicKey))],
            outputs: outputs.map { .coin($0.derivationIndex, $0.publicKey) }
        )]

        let resolved = try await rebuild.resolve([transaction], assets: assets)

        let split = try #require(resolved[transaction.id])
        #expect(split.coinToSplit.publicKey == input.publicKey)
        #expect(split.outputs.map(\.publicKey) == outputs.map(\.publicKey))
    }

    @Test("the coin being split is the only input waited on")
    func inputsAreTheSpentCoin() async throws {
        let input = RebuildFixtures.coin(1)
        let output = RebuildFixtures.coin(2)
        let rebuild = makeRebuild(coins: [input, output])
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [.coin(.own(1, input.publicKey))],
            outputs: [.coin(2, output.publicKey)]
        )]

        let split = try #require(try await rebuild.resolve([transaction], assets: assets)[transaction.id])

        #expect(rebuild.inputs(of: split) == [input.publicKey])
    }

    @Test("an output the wallet no longer holds resolves to nothing rather than a partial split")
    func missingOutputResolvesToNothing() async throws {
        let input = RebuildFixtures.coin(1)
        let present = RebuildFixtures.coin(2)
        // Coin 3 is recorded as an output but is not in the wallet.
        let rebuild = makeRebuild(coins: [input, present])
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [.coin(.own(1, input.publicKey))],
            outputs: [
                .coin(2, present.publicKey),
                .coin(3, RebuildFixtures.key(3))
            ]
        )]

        let resolved = try await rebuild.resolve([transaction], assets: assets)

        #expect(resolved[transaction.id] == nil)
    }

    @Test("a missing input resolves to nothing")
    func missingInputResolvesToNothing() async throws {
        let output = RebuildFixtures.coin(2)
        let rebuild = makeRebuild(coins: [output])
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [.coin(.own(1, RebuildFixtures.key(1)))],
            outputs: [.coin(2, output.publicKey)]
        )]

        let resolved = try await rebuild.resolve([transaction], assets: assets)

        #expect(resolved[transaction.id] == nil)
    }

    @Test("a split must spend exactly one coin")
    func severalInputsResolveToNothing() async throws {
        let first = RebuildFixtures.coin(1)
        let second = RebuildFixtures.coin(4)
        let output = RebuildFixtures.coin(2)
        let rebuild = makeRebuild(coins: [first, second, output])
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [
                .coin(.own(1, first.publicKey)),
                .coin(.own(4, second.publicKey))
            ],
            outputs: [.coin(2, output.publicKey)]
        )]

        let resolved = try await rebuild.resolve([transaction], assets: assets)

        #expect(resolved[transaction.id] == nil)
    }

    @Test("a row recording no outputs resolves to nothing")
    func noOutputsResolveToNothing() async throws {
        let input = RebuildFixtures.coin(1)
        let rebuild = makeRebuild(coins: [input])
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [.coin(.own(1, input.publicKey))],
            outputs: []
        )]

        let resolved = try await rebuild.resolve([transaction], assets: assets)

        #expect(resolved[transaction.id] == nil)
    }

    @Test("a transaction with no ledger row resolves to nothing")
    func unknownTransactionResolvesToNothing() async throws {
        let rebuild = makeRebuild(coins: [RebuildFixtures.coin(1)])
        let transaction = try scheduled()

        let resolved = try await rebuild.resolve([transaction], assets: [:])

        #expect(resolved[transaction.id] == nil)
    }

    /// The distinction the policy depends on: a row it cannot *resolve* is unbuildable and given up on,
    /// but a store that cannot be *read* says nothing about the row. Swallowing the error made the two
    /// indistinguishable and terminally failed a leg whose memo the recipient already held.
    @Test("a store that cannot be read propagates instead of resolving to nothing")
    func unreadableStorePropagates() async throws {
        let rebuild = makeRebuild(coinService: StubCoinService(fetchError: StubCoinService.Failure.unavailable))
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [.coin(.own(1, RebuildFixtures.coin(1).publicKey))],
            outputs: [.coin(2, RebuildFixtures.coin(2).publicKey)]
        )]

        await #expect(throws: StubCoinService.Failure.self) {
            _ = try await rebuild.resolve([transaction], assets: assets)
        }
    }
}

private extension SplitRebuildTests {
    func makeRebuild(coins: [Coin]) -> SplitRebuild {
        makeRebuild(coinService: StubCoinService(coins: RebuildFixtures.tracked(coins)))
    }

    func makeRebuild(coinService: any CoinServiceProtocol) -> SplitRebuild {
        SplitRebuild(
            coinService: coinService,
            coinQuery: StubCoinOnChainQuery(),
            builder: SplitExtrinsicBuilder(
                coinKeyFactory: StubCoinKeyFactory(),
                originFactory: StubOriginFactory(),
                factory: StubTxFactory(),
                chainId: "test-chain"
            )
        )
    }

    func scheduled() throws -> ScheduledDurableTx {
        try RebuildFixtures.scheduled(
            policyId: policyId,
            params: RebuildFixtures.transferParams(inSeconds: 60)
        )
    }
}
