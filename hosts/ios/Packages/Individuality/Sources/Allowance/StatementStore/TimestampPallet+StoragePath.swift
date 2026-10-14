import Foundation
import SubstrateSdk
import SubstrateSdkExt

public enum TimestampPallet {
    public static let name = "Timestamp"
}

public extension TimestampPallet {
    enum Storage {
        case now
    }
}

extension TimestampPallet.Storage: StoragePathConvertible {
    public var moduleName: String {
        TimestampPallet.name
    }

    public var name: String {
        switch self {
        case .now:
            "Now"
        }
    }
}
