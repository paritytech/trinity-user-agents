import Foundation
import StatementStore

/// Signs an unsigned statement with the given account and submits it.
public struct SigningStatementSubmitter: @unchecked Sendable {
    let connection: StatementStoreSubmitting

    public init(connection: StatementStoreSubmitting) {
        self.connection = connection
    }

    public func submit(_ statement: UnsignedStatement, signer: StatementStoreSigning) async throws {
        try await connection.submitStatement(with: statement.signed(by: signer))
    }
}
