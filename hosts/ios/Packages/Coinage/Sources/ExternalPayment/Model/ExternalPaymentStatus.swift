import Foundation
import SubstrateSdk

/// How an external payment stands, as products and the send screen report it.
public enum ExternalPaymentStatus: Sendable, Equatable {
    case processing
    case completed
    /// Terminal with a shortfall: `settledInPlanks` reached the destination, the rest never will.
    case partiallyCompleted(settledInPlanks: Balance)
    case failed(reason: String)
}

/// Whether the wallet can pay an amount out, and at what cost to privacy.
public enum ExternalPaymentPreview: Equatable, Sendable {
    /// Value that is already private covers the amount.
    case privately
    /// The amount also spends value still gaining privacy, which the user must confirm first.
    case spendsGainingPrivacy
    /// What the wallet may spend, `available` in planks, does not cover the amount.
    case notEnoughBalance(available: Balance)
}
