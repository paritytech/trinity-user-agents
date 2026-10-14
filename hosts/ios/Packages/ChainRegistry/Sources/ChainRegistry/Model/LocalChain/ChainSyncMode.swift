import Foundation

public enum ChainSyncMode: Equatable {
    case disabled
    case light
    case full
}

public extension ChainSyncMode {
    func toEntityValue() -> Int16 {
        switch self {
        case .disabled:
            0
        case .light:
            1
        case .full:
            2
        }
    }

    init?(entityValue: Int16) {
        switch entityValue {
        case 0:
            self = .disabled
        case 1:
            self = .light
        case 2:
            self = .full
        default:
            return nil
        }
    }
}
