import Foundation

/// RFC-0004 `create_proof` errors. `notMember` is distinguished from `ringNotFound`
/// so products can route users without full personhood to onboarding.
public enum CreateProofError: Error, Hashable {
    case ringNotFound
    case notMember
    case keyNotRegistered
    case keyNotInRing
    case notAllowlisted
    case rejected
    case unknown(String)

    public var code: String {
        switch self {
        case .ringNotFound: "RingNotFound"
        case .notMember: "NotMember"
        case .keyNotRegistered: "KeyNotRegistered"
        case .keyNotInRing: "KeyNotInRing"
        case .notAllowlisted: "NotAllowlisted"
        case .rejected: "Rejected"
        case .unknown: "Unknown"
        }
    }

    public var message: String {
        switch self {
        case .ringNotFound: "Ring not found for the requested location"
        case .notMember: "Selected member key is not a member of the requested ring"
        case .keyNotRegistered: "Key handle has no registry entry"
        case .keyNotInRing: "Key handle is registered, but not for the requested ring"
        case .notAllowlisted: "Key handle is foreign and its owner has not allowlisted the caller"
        case .rejected: "Proof creation was rejected"
        case let .unknown(reason): reason
        }
    }

    public static func wrapping(_ error: Error) -> CreateProofError {
        error as? CreateProofError ?? .unknown(error.localizedDescription)
    }
}

extension CreateProofError: HostCallCodedError {}
