import Foundation
import FoundationExt
import StatementStore
import SubstrateSdk

/// Not terminal: the statement is retried once consent can be asked in the foreground.
struct ChatSigningConsentUnavailableError: Error {}

/// In the background signs only with stored consent instead of waiting on an alert nobody can see.
final class ForegroundConsentSigningFallback: @unchecked Sendable {
    private let base: NonPrivateSigningFallbacking
    private let activity: any ApplicationActivityProviding

    init(base: NonPrivateSigningFallbacking, activity: any ApplicationActivityProviding) {
        self.base = base
        self.activity = activity
    }
}

extension ForegroundConsentSigningFallback: NonPrivateSigningFallbacking {
    func identitySigner(for contactAccountId: AccountId, ownSignKeyId: String) async throws -> StatementStoreSigning {
        guard await activity.isInBackground else {
            return try await base.identitySigner(for: contactAccountId, ownSignKeyId: ownSignKeyId)
        }

        guard let signer = try await base.consentedIdentitySigner(for: contactAccountId, ownSignKeyId: ownSignKeyId)
        else {
            throw ChatSigningConsentUnavailableError()
        }

        return signer
    }

    func consentedIdentitySigner(for contactAccountId: AccountId, ownSignKeyId: String) async throws
        -> StatementStoreSigning? {
        try await base.consentedIdentitySigner(for: contactAccountId, ownSignKeyId: ownSignKeyId)
    }
}
