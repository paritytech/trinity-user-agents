import BigInt
import Foundation

/// A payment the wallet can cover, before anything is reserved for it.
public struct TransferPreview: Equatable, Sendable {
    /// The requested amount, in planks.
    public let fullAmount: BigUInt
    /// The payment spends value still gaining privacy, so the user must confirm it first.
    public let spendsGainingPrivacy: Bool

    public init(fullAmount: BigUInt, spendsGainingPrivacy: Bool) {
        self.fullAmount = fullAmount
        self.spendsGainingPrivacy = spendsGainingPrivacy
    }
}

public enum TransferPreviewError: Error, Equatable {
    /// The wallet cannot cover the amount.
    case insufficientBalance
}
