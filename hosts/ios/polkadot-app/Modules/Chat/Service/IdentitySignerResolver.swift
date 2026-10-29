import Foundation
import MessageExchangeKit
import StatementStore

/// Signs with the session's own identity account: SSO and device sync.
struct IdentitySignerResolver: PeerSessionSignerResolving, @unchecked Sendable {
    let signManager: StatementStoreSignerManaging

    func signer(for request: MessageExchange.SessionRequest) async throws -> StatementStoreSigning {
        try signManager.makeSigner(for: request.own.signKeyId)
    }
}
