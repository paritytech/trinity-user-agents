import Foundation

public enum SignVrfError: Error, Hashable {
    case rejected
    case transcriptTooLarge(String)
    case unknown(String)

    public var code: String {
        switch self {
        case .rejected: "Rejected"
        case .transcriptTooLarge,
             .unknown: "Unknown"
        }
    }

    public var message: String {
        switch self {
        case .rejected: "VRF signing was rejected"
        case let .transcriptTooLarge(reason): reason
        case let .unknown(reason): reason
        }
    }

    public static func wrapping(_ error: Error) -> SignVrfError {
        error as? SignVrfError ?? .unknown(error.localizedDescription)
    }
}

extension SignVrfError: HostCallCodedError {}
