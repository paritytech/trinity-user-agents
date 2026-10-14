import Foundation
import BigInt

/// Holds a pre-computed coin selection result and the requested amount.
public struct TransferPreview {
    public let selectionResult: CoinSelectionResult
    /// The originally requested transfer amount (all coins + all vouchers).
    public let fullAmount: BigUInt
    /// The scope the plan drew on. ``SpendScope/withConfirmation`` means it spends gaining-privacy
    /// funds, so the caller must confirm before submitting.
    public let scope: SpendScope

    public init(selectionResult: CoinSelectionResult, fullAmount: BigUInt, scope: SpendScope) {
        self.selectionResult = selectionResult
        self.fullAmount = fullAmount
        self.scope = scope
    }
}
