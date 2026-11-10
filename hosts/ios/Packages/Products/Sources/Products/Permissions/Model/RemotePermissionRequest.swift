import Foundation
import SubstrateSdk

public enum RemotePermissionRequest: Equatable, Sendable {
    case remote(domains: [String])
    case webRTC
    case chainSubmit
    case preimageSubmit
    case statementSubmit

    /// Converts to domain-level permissions used by the permission guard.
    /// `Remote` expands into one `networkAccess` per domain (lowercased).
    public func toDomainPermissions() -> [ProductPermission] {
        switch self {
        case let .remote(domains):
            domains.map { .networkAccess(domain: $0.lowercased()) }
        case .webRTC:
            [.webRtcAccess]
        case .chainSubmit:
            [.chainSubmitAccess]
        case .preimageSubmit:
            [.preimageSubmitAccess]
        case .statementSubmit:
            [.statementSubmitAccess]
        }
    }
}

// MARK: - JSON Parsing

public extension RemotePermissionRequest {
    private enum Tag {
        static let remote = "Remote"
        static let webRTC = "WebRtc"
        static let chainSubmit = "ChainSubmit"
        static let preimageSubmit = "PreimageSubmit"
        static let statementSubmit = "StatementSubmit"
    }

    /// Parses a single `{ tag, value? }` JSON object into a request.
    static func from(json: JSON) throws -> RemotePermissionRequest {
        guard let tag = json["tag"]?.stringValue else {
            throw RemotePermissionRequestError.missingTag
        }

        switch tag {
        case Tag.remote:
            let domains = json["value"]?.arrayValue?.compactMap(\.stringValue) ?? []
            return .remote(domains: domains)
        case Tag.webRTC:
            return .webRTC
        case Tag.chainSubmit:
            return .chainSubmit
        case Tag.preimageSubmit:
            return .preimageSubmit
        case Tag.statementSubmit:
            return .statementSubmit
        default:
            throw RemotePermissionRequestError.unknownTag(tag)
        }
    }

    /// Parses a JSON array of permission objects.
    static func fromArray(json: JSON) throws -> [RemotePermissionRequest] {
        guard let array = json.arrayValue else {
            throw RemotePermissionRequestError.expectedArray
        }
        return try array.map { try from(json: $0) }
    }
}

public enum RemotePermissionRequestError: Error {
    case missingTag
    case unknownTag(String)
    case expectedArray
}
