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
    public static let userIdentityAccessTypeName = "user_identity_access"
    public static let chatAuthorityTypeName = "chat_authority"
    public static let profileDisclosureTypeName = "profile_disclosure"
    public static let statementStoreAllowanceTypeName = "statement_store_allowance"

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
    case userIdentityAccess
    case chatAuthority
    case profileDisclosure
    /// `nil` is the legacy allowance account, distinct from every product selector.
    case statementStoreAllowance(derivationIndex: ProductAccountSelector?)

    /// Whether this is one of the core's `RemotePermission` cases: a product's
    /// own outbound access.
    ///
    /// The distinction exists because the core grants a first-party product
    /// every remote permission without prompting, and nothing else. Device
    /// capabilities, account access, balance, identity disclosure, Chat authority,
    /// profile disclosure and Statement Store allowance always prompt, whoever asks,
    /// so they must not ride along on that trust.
    public var isRemoteAccess: Bool {
        switch self {
        case .networkAccess, .networkAccessBundle, .webRtcAccess, .chainSubmitAccess, .preimageSubmitAccess,
             .statementSubmitAccess:
            true
        case .deviceCapability, .accountAccess, .balanceAccess, .userIdentityAccess,
             .chatAuthority, .profileDisclosure, .statementStoreAllowance:
            false
        }
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
        case .userIdentityAccess:
            Self.userIdentityAccessTypeName
        case .chatAuthority:
            Self.chatAuthorityTypeName
        case .profileDisclosure:
            Self.profileDisclosureTypeName
        case .statementStoreAllowance:
            Self.statementStoreAllowanceTypeName
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
        case let .statementStoreAllowance(derivationIndex):
            switch derivationIndex {
            case nil:
                "legacy"
            case let .index(index):
                "index:\(index)"
            case let .raw(bytes):
                "raw:\(bytes.base64EncodedString())"
            }
        case .balanceAccess,
             .webRtcAccess,
             .chainSubmitAccess,
             .preimageSubmitAccess,
             .statementSubmitAccess,
             .userIdentityAccess,
             .chatAuthority,
             .profileDisclosure:
            ""
        }
    }

    /// Reconstruct a permission from its persisted `(typeName, key)` pair.
    /// Returns `nil` for unknown type names or malformed capability/selector keys.
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
        case userIdentityAccessTypeName:
            return .userIdentityAccess
        case chatAuthorityTypeName:
            return .chatAuthority
        case profileDisclosureTypeName:
            return .profileDisclosure
        case statementStoreAllowanceTypeName:
            if key == "legacy" {
                return .statementStoreAllowance(derivationIndex: nil)
            }
            if key.hasPrefix("index:"), let index = UInt32(key.dropFirst(6)) {
                return .statementStoreAllowance(derivationIndex: .index(index))
            }
            if key.hasPrefix("raw:"), let bytes = Data(base64Encoded: String(key.dropFirst(4))),
               bytes.count == 32 {
                return .statementStoreAllowance(derivationIndex: .raw(bytes))
            }
            return nil
        default:
            return nil
        }
    }
}
