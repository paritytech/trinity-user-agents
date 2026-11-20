import Foundation
import SubstrateSdkExt

public extension ResourcesPallet {
    enum Constants {
        case longTermStoragePeriodDuration
        /// Exported by old runtimes; later ones keep it in `pallet_parameters`.
        case notificationAllowance
        case notificationPeriodDuration
    }
}

extension ResourcesPallet.Constants: ConstantPathConvertible {
    public var name: String {
        switch self {
        case .longTermStoragePeriodDuration:
            "LongTermStoragePeriodDuration"
        case .notificationAllowance:
            "NotificationAllowance"
        case .notificationPeriodDuration:
            "NotificationPeriodDuration"
        }
    }

    public var moduleName: String { ResourcesPallet.name }
}
