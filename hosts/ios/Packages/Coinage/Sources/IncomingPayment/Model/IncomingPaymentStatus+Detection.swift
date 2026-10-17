import Foundation
import SubstrateSdk

public extension IncomingPaymentStatus {
    /// Values claim progress against the payment's immutable minimum before discarding the raw
    /// credit. Zero claims all and requires positive finalized credit. Only a terminal source
    /// verdict can satisfy the minimum; interim partial progress is never success.
    init(detection: CoinageTransferDetection, amount: Balance) {
        switch detection {
        case .detecting:
            self = .detecting
        case .claiming,
             .claimingRest:
            self = .claiming
        case let .claimed(actual, finalized):
            if actual > 0, actual >= amount {
                self = .claimed(finalized: finalized)
            } else if finalized {
                self = actual > 0 ? .claimedPartially(actualClaimed: actual) : .notClaimed
            } else {
                self = .claiming
            }
        case let .claimedPartially(actual):
            if actual > 0, actual >= amount {
                self = .claimed(finalized: true)
            } else {
                self = actual > 0 ? .claimedPartially(actualClaimed: actual) : .notClaimed
            }
        case .notClaimed:
            self = .notClaimed
        }
    }
}
