import Foundation

/// Errors that can occur during coin selection.
public enum CoinSelectionError: Error, Equatable {
    /// The wallet does not have enough funds to cover the requested amount.
    case insufficientFunds
    /// The requested amount cannot be represented with available denominations.
    case amountNotRepresentable
    /// The requested amount is zero.
    case zeroAmount
    /// The wallet contains no coins or vouchers.
    case emptyWallet
    /// A single voucher's unload already mints more coins than the pallet allows, so there is no
    /// smaller call left to split into. Only reachable on a chain whose `MaxSplitOutputs` is
    /// smaller than its denomination range.
    case unloadOutputsExceedLimit(outputs: Int, max: Int)
}
