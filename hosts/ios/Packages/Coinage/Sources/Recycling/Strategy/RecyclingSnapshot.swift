import Foundation
import SubstrateSdk

/// The value aggregate for one evaluation. `unavailable` is the pending bucket *before* this pass
/// gates anything; the policy accumulates into it, so the resulting pending equals the accumulated
/// value and the budget invariant holds by construction.
public struct RecyclingSnapshot: Equatable {
    /// Everything the user holds that is free: active coins plus free vouchers.
    public let total: Balance
    /// Already-unavailable value (still-arriving coins and vouchers, plus vouchers gaining privacy).
    public let unavailable: Balance

    public init(total: Balance, unavailable: Balance) {
        self.total = total
        self.unavailable = unavailable
    }
}
