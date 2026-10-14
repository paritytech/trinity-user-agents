import Foundation

/// Which funds a spend may draw on.
public enum SpendScope: Sendable {
    /// Freely spendable now: `allowUse` coins and usable vouchers.
    case spendable
    /// Adds gaining-privacy funds behind a confirmation — but only when the strategy allows it, so it
    /// can never override `maxPrivacy` (where it equals ``spendable``).
    case withConfirmation
}
