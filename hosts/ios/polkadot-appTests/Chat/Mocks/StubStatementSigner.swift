import Foundation
import StatementStore
import SubstrateSdk

/// Signs with a constant signature; tests compare only the account.
struct StubStatementSigner: StatementStoreSigning {
    let accountId: AccountId

    func sign(_: Data) throws -> StatementProof {
        .sr25519(signature: Data(repeating: 0, count: StatementProof.signatureSize), signer: accountId)
    }
}
