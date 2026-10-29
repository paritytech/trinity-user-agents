import Foundation
import Operation_iOS
import SubstrateSdk

extension Chat {
    /// The user agreed that statements toward this contact may be signed by their identity account.
    struct IdentitySigningConsent: Identifiable {
        let contactAccountId: AccountId
        let grantedAt: Date

        var identifier: String { contactAccountId.toHex() }
    }
}
