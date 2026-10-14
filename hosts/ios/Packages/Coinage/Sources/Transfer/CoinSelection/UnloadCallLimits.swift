import Foundation

/// What the pallet allows a single unload call to carry.
///
/// Both are read from chain constants rather than assumed: a recycler holding more vouchers than
/// `maxVouchersPerCall`, or a budget that breaks into more coins than `maxOutputsPerCall`, is
/// unloaded by several calls instead of being rejected.
struct UnloadCallLimits: Equatable {
    /// `MaxConsolidation`: vouchers (aliases) one call may unload.
    let maxVouchersPerCall: Int

    /// `MaxSplitOutputs`: coins one call may mint, recipient and change counted together.
    let maxOutputsPerCall: Int
}
