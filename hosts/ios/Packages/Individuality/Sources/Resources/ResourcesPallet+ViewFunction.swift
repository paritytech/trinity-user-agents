import Foundation
import SubstrateSdkExt

public extension ResourcesPallet {
    enum ViewFunction {
        case stmtStoreSlotsPerPeriod
        case liteStmtStoreSlotsPerPeriod
        case stmtStoreReplacementCooldown
        case longTermStorageClaimsPerPeriod
        /// Highest claimable notification seq of a full person; seqs `0...value` are valid.
        case notificationSlotsPerPeriod
        /// Highest claimable notification seq of a lite person; seqs `0...value` are valid.
        case liteNotificationSlotsPerPeriod
    }
}

extension ResourcesPallet.ViewFunction: ViewFunctionCallConvertible {
    public var name: String {
        switch self {
        case .stmtStoreSlotsPerPeriod:
            "get_stmt_store_slots_per_period"
        case .liteStmtStoreSlotsPerPeriod:
            "get_lite_stmt_store_slots_per_period"
        case .stmtStoreReplacementCooldown:
            "get_stmt_store_replacement_cooldown"
        case .longTermStorageClaimsPerPeriod:
            "get_long_term_storage_claims_per_period"
        case .notificationSlotsPerPeriod:
            "get_notification_slots_per_period"
        case .liteNotificationSlotsPerPeriod:
            "get_lite_notification_slots_per_period"
        }
    }

    public var moduleName: String { ResourcesPallet.name }
}
