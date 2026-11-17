import Foundation

public enum ListRingVrfKeysError: Error, Hashable {
    case notConnected
    case rejected
    case unknown(String)

    public var code: String {
        switch self {
        case .notConnected: "NotConnected"
        case .rejected: "Rejected"
        case .unknown: "Unknown"
        }
    }

    public var message: String {
        switch self {
        case .notConnected: "Account is not connected"
        case .rejected: "Listing ring-VRF keys of another product was rejected"
        case let .unknown(reason): reason
        }
    }

    public static func wrapping(_ error: Error) -> ListRingVrfKeysError {
        error as? ListRingVrfKeysError ?? .unknown(error.localizedDescription)
    }
}

extension ListRingVrfKeysError: HostCallCodedError {}
