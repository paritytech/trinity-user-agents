import Foundation

/// RFC-0004 `get_alias` errors. `get_alias` and `create_proof` perform the same ring
/// resolution and member-key selection, so they carry identical error sets — but the
/// spec keeps them as distinct types so the two boundaries can evolve independently.
public enum GetAliasError: Error, Hashable {
    case ringNotFound
    case notMember
    case rejected
    case unknown(String)

    public var code: String {
        switch self {
        case .ringNotFound: "RingNotFound"
        case .notMember: "NotMember"
        case .rejected: "Rejected"
        case .unknown: "Unknown"
        }
    }

    public var message: String {
        switch self {
        case .ringNotFound: "Ring not found for the requested location"
        case .notMember: "Selected member key is not a member of the requested ring"
        case .rejected: "Alias derivation was rejected"
        case let .unknown(reason): reason
        }
    }

    public static func wrapping(_ error: Error) -> GetAliasError {
        error as? GetAliasError ?? .unknown(error.localizedDescription)
    }
}

extension GetAliasError: HostCallCodedError {}
