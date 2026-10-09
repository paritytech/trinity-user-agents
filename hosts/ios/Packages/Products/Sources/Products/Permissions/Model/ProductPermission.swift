import Foundation

/// Permission a product can request from the host.
///
/// `typeName` / `key` form the composite identity used in the persistence layer.
public enum ProductPermission: Equatable, Sendable {
    public static let deviceCapabilityTypeName = "device_capability"
    public static let networkAccessTypeName = "network_access"
    public static let networkAccessBundleTypeName = "network_access_bundle"
    public static let accountAccessTypeName = "account_access"
    public static let webRtcAccessTypeName = "webrtc_access"
    public static let chainSubmitAccessTypeName = "chain_submit"
    public static let preimageSubmitAccessTypeName = "preimage_submit"
    public static let balanceAccessTypeName = "balance_access"
    public static let statementSubmitAccessTypeName = "statement_submit"
    public static let jamPeersAccessTypeName = "jam_peers"
    public static let userIdentityAccessTypeName = "user_identity_access"

    case deviceCapability(DeviceCapabilityType)
    case networkAccess(domain: String)
    /// Exact native domain-set decision; not equivalent to separate domain decisions.
    case networkAccessBundle(domains: [String])
    case accountAccess(targetProductId: String)
    case balanceAccess
    case webRtcAccess
    case chainSubmitAccess
    case preimageSubmitAccess
    case statementSubmitAccess
    /// Peer messaging access to the JAM network whose genesis header hash is
    /// `genesis` (lowercase `0x`-prefixed hex).
    case jamPeersAccess(genesis: String)
    case userIdentityAccess

    /// Whether this is one of the core's `RemotePermission` cases: a product's
    /// own outbound access.
    ///
    /// The distinction exists because the core grants a first-party product
    /// every remote permission without prompting, and nothing else. Device
    /// capabilities, account access, balance and identity disclosure always
    /// prompt, whoever asks, so they must not ride along on that trust.
    public var isRemoteAccess: Bool {
        switch self {
        case .networkAccess, .networkAccessBundle, .webRtcAccess, .chainSubmitAccess, .preimageSubmitAccess,
             .statementSubmitAccess, .jamPeersAccess:
            true
        case .deviceCapability, .accountAccess, .balanceAccess, .userIdentityAccess:
            false
        }
    }

    /// A JAM genesis as shown to the user: `0x`, its first 8 hex digits and `…`.
    public static func shortGenesis(_ genesis: String) -> String {
        let digits = genesis.hasPrefix("0x") ? genesis.dropFirst(2) : Substring(genesis)
        return "0x\(digits.prefix(8))…"
    }

    public var typeName: String {
        switch self {
        case .deviceCapability:
            Self.deviceCapabilityTypeName
        case .networkAccess:
            Self.networkAccessTypeName
        case .networkAccessBundle:
            Self.networkAccessBundleTypeName
        case .accountAccess:
            Self.accountAccessTypeName
        case .balanceAccess:
            Self.balanceAccessTypeName
        case .webRtcAccess:
            Self.webRtcAccessTypeName
        case .chainSubmitAccess:
            Self.chainSubmitAccessTypeName
        case .preimageSubmitAccess:
            Self.preimageSubmitAccessTypeName
        case .statementSubmitAccess:
            Self.statementSubmitAccessTypeName
        case .jamPeersAccess:
            Self.jamPeersAccessTypeName
        case .userIdentityAccess:
            Self.userIdentityAccessTypeName
        }
    }

    public var key: String {
        switch self {
        case let .deviceCapability(capability):
            capability.rawValue
        case let .networkAccess(domain):
            domain
        case let .networkAccessBundle(domains):
            domains.joined(separator: "\n")
        case let .accountAccess(targetProductId):
            targetProductId
        case let .jamPeersAccess(genesis):
            genesis
        case .balanceAccess,
             .webRtcAccess,
             .chainSubmitAccess,
             .preimageSubmitAccess,
             .statementSubmitAccess,
             .userIdentityAccess:
            ""
        }
    }

    /// Reconstruct a permission from its persisted `(typeName, key)` pair.
    /// Returns `nil` for unknown type names or malformed device capability keys.
    public static func from(typeName: String, key: String) -> ProductPermission? {
        switch typeName {
        case deviceCapabilityTypeName:
            guard let capability = DeviceCapabilityType(rawValue: key) else { return nil }
            return .deviceCapability(capability)
        case networkAccessTypeName:
            return .networkAccess(domain: key)
        case networkAccessBundleTypeName:
            return .networkAccessBundle(domains: key.components(separatedBy: "\n"))
        case accountAccessTypeName:
            return .accountAccess(targetProductId: key)
        case balanceAccessTypeName:
            return .balanceAccess
        case webRtcAccessTypeName:
            return .webRtcAccess
        case chainSubmitAccessTypeName:
            return .chainSubmitAccess
        case preimageSubmitAccessTypeName:
            return .preimageSubmitAccess
        case statementSubmitAccessTypeName:
            return .statementSubmitAccess
        case jamPeersAccessTypeName:
            return .jamPeersAccess(genesis: key)
        case userIdentityAccessTypeName:
            return .userIdentityAccess
        default:
            return nil
        }
    }
}
