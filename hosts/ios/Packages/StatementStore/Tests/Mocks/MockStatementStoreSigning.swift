import Foundation
import SubstrateSdk
@testable import StatementStore

final class MockStatementStoreSigning: StatementStoreSigning {
    let accountId: AccountId

    private(set) var signedDataEntries: [Data] = []

    init(accountId: Data = Data(repeating: 0xAA, count: 32)) {
        self.accountId = accountId
    }

    func sign(_ data: Data) throws -> StatementProof {
        signedDataEntries.append(data)

        return .sr25519(
            signature: Data(repeating: 0xBB, count: StatementProof.signatureSize),
            signer: accountId
        )
    }
}
