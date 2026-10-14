import Foundation
import SubstrateSdkExt

public enum NetworkSuffixPallet {
    public static let name = "NetworkSuffix"

    public enum Storage {
        case networkSuffix
    }
}

extension NetworkSuffixPallet.Storage: StoragePathConvertible {
    public var moduleName: String { NetworkSuffixPallet.name }

    public var name: String {
        switch self {
        case .networkSuffix:
            "NetworkSuffix"
        }
    }
}
