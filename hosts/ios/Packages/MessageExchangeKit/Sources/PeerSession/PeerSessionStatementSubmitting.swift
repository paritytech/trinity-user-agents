import Foundation
import StatementStore

/// Signs and submits a session's statements; the app decides which account signs for each use case.
public protocol PeerSessionStatementSubmitting: Sendable {
    func submit(_ statement: UnsignedStatement) async throws
}

/// Picks the account that signs a session's next statement.
public protocol PeerSessionSignerResolving: Sendable {
    func signer(for request: MessageExchange.SessionRequest) async throws -> StatementStoreSigning
}

struct ResolvingStatementSubmitter: PeerSessionStatementSubmitting, @unchecked Sendable {
    let connection: StatementStoreSubmitting
    let signerResolver: PeerSessionSignerResolving
    let request: MessageExchange.SessionRequest

    func submit(_ statement: UnsignedStatement) async throws {
        let signer = try await signerResolver.signer(for: request)
        try await connection.submitStatement(with: statement.signed(by: signer))
    }
}
