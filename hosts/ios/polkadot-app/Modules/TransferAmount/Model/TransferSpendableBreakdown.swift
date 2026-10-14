import Foundation
import BigInt

/// The enter-amount balance split: what can be spent at no privacy cost, and the extra that is
/// reachable only behind a confirmation.
struct TransferSpendableBreakdown: Equatable {
    /// Spendable now with no privacy cost — the `Max:` figure.
    let availablePrivate: BigUInt
    /// Held back for privacy; reachable only with a confirmation (zero when the strategy forbids it).
    let gainingPrivacy: BigUInt
}
