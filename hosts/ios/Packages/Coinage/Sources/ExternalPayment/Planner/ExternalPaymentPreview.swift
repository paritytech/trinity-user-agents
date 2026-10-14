import Foundation
import SubstrateSdk

/// Vouchers picked for an unload and what they exceed the target by; the surplus is folded back into
/// fresh vouchers by the unload itself.
public struct VoucherOffboarding: Equatable {
    public let vouchers: [TrackedVoucher]
    public let surplus: Balance

    public init(vouchers: [TrackedVoucher], surplus: Balance) {
        self.vouchers = vouchers
        self.surplus = surplus
    }
}

///  Whether the plan costs privacy is a separate question (`canPayPrivately`), not encoded here.
public enum ExternalPaymentPreview: Equatable {
    /// Vouchers already in a recycler cover the amount; they are unloaded as picked.
    case unloadVouchers(VoucherOffboarding)
    /// `coins` are recycled first, then unloaded together with `exactVouchers`, which are every
    /// voucher the chain would accept.
    case loadCoins(coins: [TrackedCoin], exactVouchers: [TrackedVoucher])
    /// What the chain would accept cannot cover the amount.
    case notEnoughBalance

    public var vouchers: [TrackedVoucher] {
        switch self {
        case let .unloadVouchers(offboarding):
            offboarding.vouchers
        case let .loadCoins(_, vouchers):
            vouchers
        case .notEnoughBalance:
            []
        }
    }

    public var coins: [TrackedCoin] {
        if case let .loadCoins(coins, _) = self { return coins }
        return []
    }
}
