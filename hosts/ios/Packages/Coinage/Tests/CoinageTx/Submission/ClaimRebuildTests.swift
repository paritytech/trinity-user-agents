import DurableTransactions
import Foundation
import NovaCrypto
import Testing
@testable import Coinage

/// A claim rebuilt from its ledger row must mint into the *same* coin its first attempt recorded: a
/// payment already registered against that coin is waiting on it, and a claim retried into a fresh coin
/// would leave that payment waiting for ever.
@Suite("Claim Rebuild")
struct ClaimRebuildTests {
    private let policyId = CoinageSubmissionParams.claimPolicyId
    /// A real coin secret: 64 bytes, and canonical — an arbitrary 64 bytes is not a valid scalar and
    /// the key factory refuses it, so the secret is derived the way the wallet derives one.
    private let secret = try! SNKeyFactory()
        .createKeypair(fromSeed: Data(repeating: 0xAB, count: 32))
        .privateKey()
        .rawData()

    @Test("terms come from the claim params")
    func termsAreDecoded() throws {
        let rebuild = makeRebuild()

        let terms = try #require(
            rebuild.terms(of: RebuildFixtures.claimParams(inSeconds: 60, receivedKey: secret))
        )

        #expect(terms.deadline == RebuildFixtures.now.addingTimeInterval(60))
        // A claim's failures are always worth weighing: only the window ends its rebuilds.
        #expect(terms.retriesFailures)
    }

    @Test("params that cannot be read have no terms")
    func unreadableParamsHaveNoTerms() {
        let rebuild = makeRebuild()

        #expect(rebuild.terms(of: Data([0xFF])) == nil)
    }

    @Test("the claim resolves into the coin the row already recorded")
    func resolvesIntoTheRecordedCoin() async throws {
        let rebuild = makeRebuild()
        let destination = RebuildFixtures.coin(7)
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [.coin(.received(RebuildFixtures.key(1)))],
            outputs: [.coin(7, destination.publicKey)]
        )]

        let claim = try #require(try await rebuild.resolve([transaction], assets: assets)[transaction.id])

        #expect(claim.destination == destination.publicKey)
        #expect(claim.receivedKey == secret)
    }

    @Test("the source waited on is the key the params carry, not one from the ledger")
    func sourceComesFromTheParams() async throws {
        let rebuild = makeRebuild()
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [.coin(.received(RebuildFixtures.key(1)))],
            outputs: [.coin(7, RebuildFixtures.key(7))]
        )]

        let claim = try #require(try await rebuild.resolve([transaction], assets: assets)[transaction.id])

        let expected = try SNKeyFactory().createPublicKey(fromSecret: secret).rawData()
        #expect(claim.source == expected)
        #expect(rebuild.inputs(of: claim) == [expected])
    }

    @Test("a row with no recorded output resolves to nothing")
    func noOutputResolvesToNothing() async throws {
        let rebuild = makeRebuild()
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [.coin(.received(RebuildFixtures.key(1)))],
            outputs: []
        )]

        let resolved = try await rebuild.resolve([transaction], assets: assets)

        #expect(resolved[transaction.id] == nil)
    }

    @Test("a transaction with no ledger row resolves to nothing")
    func unknownTransactionResolvesToNothing() async throws {
        let rebuild = makeRebuild()
        let transaction = try scheduled()

        let resolved = try await rebuild.resolve([transaction], assets: [:])

        #expect(resolved[transaction.id] == nil)
    }

    @Test("a key no public key can be derived from resolves to nothing")
    func underivableKeyResolvesToNothing() async throws {
        let rebuild = makeRebuild()
        // A 32-byte value is a seed, not a secret, and is refused by the key factory.
        let transaction = try RebuildFixtures.scheduled(
            policyId: policyId,
            params: RebuildFixtures.claimParams(
                inSeconds: 60,
                receivedKey: Data(repeating: 0x01, count: 32)
            )
        )
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [.coin(.received(RebuildFixtures.key(1)))],
            outputs: [.coin(7, RebuildFixtures.key(7))]
        )]

        let resolved = try await rebuild.resolve([transaction], assets: assets)

        #expect(resolved[transaction.id] == nil)
    }
}

private extension ClaimRebuildTests {
    func makeRebuild() -> ClaimRebuild {
        ClaimRebuild(
            coinQuery: StubCoinOnChainQuery(),
            builder: ClaimExtrinsicBuilder(
                originFactory: StubOriginFactory(),
                factory: StubTxFactory(),
                chainId: "test-chain"
            ),
            snKeyFactory: SNKeyFactory()
        )
    }

    func scheduled() throws -> ScheduledDurableTx {
        try RebuildFixtures.scheduled(
            policyId: policyId,
            params: RebuildFixtures.claimParams(inSeconds: 60, receivedKey: secret)
        )
    }
}
