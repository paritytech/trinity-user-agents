import DurableTransactions
import Foundation
import Testing
@testable import Coinage

/// A reclaim rebuilds from its ledger row into the coin its first attempt recorded, signed by the key of the
/// coin it spends. The rebuild finds that coin in the ledger rather than trusting the payment memo.
@Suite("Reclaim Rebuild")
struct ReclaimRebuildTests {
    private let policyId = CoinageSubmissionParams.reclaimPolicyId
    private let spent = RebuildFixtures.coin(1)
    private let destination = RebuildFixtures.coin(7)

    @Test("terms come from the transfer params")
    func termsAreDecoded() throws {
        let rebuild = makeRebuild(coins: [])

        let terms = try #require(
            rebuild.terms(of: RebuildFixtures.transferParams(inSeconds: 60, retryFailures: true))
        )

        #expect(terms.deadline == RebuildFixtures.now.addingTimeInterval(60))
        #expect(terms.retriesFailures)
    }

    @Test("a reclaim resolves into the coin it spends and the coin it mints")
    func resolvesIntoSpentAndMintedCoin() async throws {
        let rebuild = makeRebuild(coins: [spent])
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [.coin(.reclaimed(spent.derivationIndex, spent.publicKey))],
            outputs: [.coin(destination.derivationIndex, destination.publicKey)]
        )]

        let reclaim = try #require(try await rebuild.resolve([transaction], assets: assets)[transaction.id])

        #expect(reclaim.coin == spent)
        #expect(reclaim.destination == destination.publicKey)
    }

    @Test(
        "a row that is not one reclaimed coin in and one coin out resolves to nothing",
        arguments: UnbuildableShape.allCases
    )
    func unbuildableShapesResolveToNothing(shape: UnbuildableShape) async throws {
        let rebuild = makeRebuild(coins: [spent])
        let transaction = try scheduled()
        let mintedOutput = OwnAsset.coin(destination.derivationIndex, destination.publicKey)
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: shape.inputs(spending: spent),
            outputs: shape.outputs(minting: mintedOutput)
        )]

        let resolved = try await rebuild.resolve([transaction], assets: assets)

        #expect(resolved[transaction.id] == nil)
    }

    @Test("a reclaim whose coin is not in the ledger resolves to nothing")
    func missingCoinResolvesToNothing() async throws {
        let rebuild = makeRebuild(coins: [])
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [.coin(.reclaimed(spent.derivationIndex, spent.publicKey))],
            outputs: [.coin(destination.derivationIndex, destination.publicKey)]
        )]

        let resolved = try await rebuild.resolve([transaction], assets: assets)

        #expect(resolved[transaction.id] == nil)
    }

    @Test("the reclaim is signed with the key derived for the coin it spends")
    func buildSignsWithOwnDerivedKey() async throws {
        let keys = StubCoinKeyFactory()
        let rebuild = makeRebuild(coins: [spent], keys: keys)
        let reclaim = ReclaimRebuild.Reclaim(coin: spent, destination: destination.publicKey)

        _ = try await rebuild.build([reclaim])

        #expect(keys.derivedPrivateKeyIndices == [spent.derivationIndex])
    }
}

extension ReclaimRebuildTests {
    enum UnbuildableShape: CaseIterable {
        case noInput
        case twoInputs
        case voucherInput
        case ownInput
        case noOutput
        case twoOutputs

        func inputs(spending spent: Coin) -> [CoinageTxInput] {
            let reclaimed = CoinageTxInput.coin(.reclaimed(spent.derivationIndex, spent.publicKey))
            return switch self {
            case .noInput: []
            case .twoInputs: [reclaimed, reclaimed]
            case .voucherInput: [.recyclerVoucher(3, RebuildFixtures.key(3))]
            case .ownInput: [.coin(.own(spent.derivationIndex, spent.publicKey))]
            case .noOutput,
                 .twoOutputs: [reclaimed]
            }
        }

        func outputs(minting minted: OwnAsset) -> [OwnAsset] {
            switch self {
            case .noOutput: []
            case .twoOutputs: [minted, minted]
            case .noInput,
                 .twoInputs,
                 .voucherInput,
                 .ownInput: [minted]
            }
        }
    }
}

private extension ReclaimRebuildTests {
    func makeRebuild(coins: [Coin], keys: any CoinKeyDeriving = StubCoinKeyFactory()) -> ReclaimRebuild {
        ReclaimRebuild(
            coinService: StubCoinService(coins: RebuildFixtures.tracked(coins)),
            coinQuery: StubCoinOnChainQuery(),
            coinKeyFactory: keys,
            builder: ClaimExtrinsicBuilder(
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
