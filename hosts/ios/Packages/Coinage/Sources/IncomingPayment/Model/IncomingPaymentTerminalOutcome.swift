import Foundation
import SubstrateSdk

/// The persisted, immutable verdict of a settled incoming payment.
///
/// Written once, on reaching a terminal status, and read back **exactly** — never re-derived — so a
/// reorg after settlement can never change what a completed top-up reports. Only a finalized full
/// claim is ever recorded as `.claimed`.
public enum IncomingPaymentTerminalOutcome: Equatable, Sendable {
    /// The full amount was claimed and finalized.
    case claimed
    /// Only `actualClaimed` (< amount) was ever claimed.
    case claimedPartially(actualClaimed: Balance)
    /// Nothing was claimed.
    case notClaimed
}
