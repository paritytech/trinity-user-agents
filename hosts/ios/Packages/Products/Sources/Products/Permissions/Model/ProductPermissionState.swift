import Foundation

public enum ProductPermissionState: Equatable {
    case allowedOnce
    case allowedAlways
    case denied
    case notDetermined

    public var isAllowed: Bool {
        switch self {
        case .allowedAlways,
             .allowedOnce:
            true
        default:
            false
        }
    }
}
