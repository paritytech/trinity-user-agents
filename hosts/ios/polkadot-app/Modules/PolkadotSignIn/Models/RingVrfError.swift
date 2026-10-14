import Foundation
import Products
import SubstrateSdk

/// RFC-0004 SSO-channel failure for ring-VRF alias and proof responses. `get_alias`
/// and `create_proof` perform the same ring resolution and member-key selection, so
/// both responses carry this enum. The SCALE variant order is the cross-host wire contract.
enum RingVrfError: Error, Hashable {
    case ringNotFound
    case notMember
    case rejected
    case unknown(String)
}

extension RingVrfError {
    /// Collapse a handler error (`CreateProofError`, `GetAliasError`, or anything else)
    /// into the wire enum. Known handler errors map exhaustively so a new case fails
    /// compilation here instead of silently degrading to `.unknown`.
    static func wrapping(_ error: Error) -> RingVrfError {
        switch error {
        case let error as RingVrfError:
            error
        case let error as CreateProofError:
            wrapping(createProofError: error)
        case let error as GetAliasError:
            wrapping(getAliasError: error)
        case let coded as HostCallCodedError:
            .unknown(coded.message)
        default:
            .unknown(error.localizedDescription)
        }
    }
}

private extension RingVrfError {
    static func wrapping(createProofError: CreateProofError) -> RingVrfError {
        switch createProofError {
        case .ringNotFound: .ringNotFound
        case .notMember: .notMember
        case .rejected: .rejected
        case let .unknown(reason): .unknown(reason)
        }
    }

    static func wrapping(getAliasError: GetAliasError) -> RingVrfError {
        switch getAliasError {
        case .ringNotFound: .ringNotFound
        case .notMember: .notMember
        case .rejected: .rejected
        case let .unknown(reason): .unknown(reason)
        }
    }
}

extension RingVrfError: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case 0: self = .ringNotFound
        case 1: self = .notMember
        case 2: self = .rejected
        case 3: self = try .unknown(String(scaleDecoder: scaleDecoder))
        default: throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case .ringNotFound:
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
        case .notMember:
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
        case .rejected:
            try UInt8(2).encode(scaleEncoder: scaleEncoder)
        case let .unknown(reason):
            try UInt8(3).encode(scaleEncoder: scaleEncoder)
            try reason.encode(scaleEncoder: scaleEncoder)
        }
    }
}
