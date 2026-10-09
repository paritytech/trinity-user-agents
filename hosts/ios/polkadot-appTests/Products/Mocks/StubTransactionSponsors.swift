import Foundation
import Products
import KeyDerivation
import SubstrateSdk

/// Sponsors nothing: pGas sponsoring is a no-op and the wallet sponsors throw.
final class StubTransactionSponsors: TransactionSponsorMaking, PGasTransactionSponsoring,
    PreimageSubmitSponsoring, StatementStoreSponsoring {
    struct Unsupported: Error {}

    func makePreimageSponsor() -> PreimageSubmitSponsoring { self }
    func makeStatementStoreSponsor() -> StatementStoreSponsoring { self }
    func makePGasSponsor() -> PGasTransactionSponsoring { self }

    func sponsorIfNeeded(productAccount _: ProductAccountId, callData _: Data, chainId _: ChainId) async throws {}

    func sponsor(productId _: ProductId, data _: Data) async throws -> any WalletManaging {
        throw Unsupported()
    }

    func sponsor(productId _: ProductId) async throws -> any WalletManaging {
        throw Unsupported()
    }
}
