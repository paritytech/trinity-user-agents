import struct TrUAPIHost.ProductId
import enum TrUAPIHost.CoinPaymentPurseAction
import Foundation

/// Permission a product can request from the host.
///
/// `typeName` / `key` form the composite identity used in the persistence layer.
public enum ProductPermission: Equatable, Sendable {
    public static let deviceCapabilityTypeName = "device_capability"
    public static let networkAccessTypeName = "network_access"
    public static let accountAccessTypeName = "account_access"
    public static let webRtcAccessTypeName = "webrtc_access"
    public static let chainSubmitAccessTypeName = "chain_submit"
    public static let preimageSubmitAccessTypeName = "preimage_submit"
    public static let balanceAccessTypeName = "balance_access"
    public static let statementSubmitAccessTypeName = "statement_submit"
    public static let userIdentityAccessTypeName = "user_identity_access"
    public static let nftPurseReadTypeName = "nft_purse_read"
    public static let coinPaymentTypeName = "coin_payment"
    public static let coinPaymentPurseTypeName = "coin_payment_purse"

    case deviceCapability(DeviceCapabilityType)
    case networkAccess(domain: String)
    case accountAccess(targetProductId: ProductId)
    case balanceAccess
    case webRtcAccess
    case chainSubmitAccess
    case preimageSubmitAccess
    case statementSubmitAccess
    case userIdentityAccess
    /// Reading the NFT purses of another owner: a product, or the user's own account when `nil`.
    case nftPurseRead(ownerProductId: ProductId?)
    /// Paying and receiving with coinage purses at all.
    case coinPayment
    /// One action on one coinage purse another product created, or on the main purse.
    case coinPaymentPurse(purse: UInt32, action: CoinPaymentPurseAction)

    /// Whether this is one of the core's `RemotePermission` cases: a product's
    /// own outbound access.
    ///
    /// The distinction exists because the core grants a first-party product
    /// every remote permission without prompting, and nothing else. Device
    /// capabilities, account access, balance and identity disclosure always
    /// prompt, whoever asks, so they must not ride along on that trust.
    public var isRemoteAccess: Bool {
        switch self {
        case .networkAccess,
             .webRtcAccess,
             .chainSubmitAccess,
             .preimageSubmitAccess,
             .statementSubmitAccess:
            true
        case .deviceCapability,
             .accountAccess,
             .balanceAccess,
             .userIdentityAccess,
             .nftPurseRead,
             .coinPayment,
             .coinPaymentPurse:
            false
        }
    }

    public var typeName: String {
        switch self {
        case .deviceCapability:
            Self.deviceCapabilityTypeName
        case .networkAccess:
            Self.networkAccessTypeName
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
        case .nftPurseRead:
            Self.nftPurseReadTypeName
        case .coinPayment:
            Self.coinPaymentTypeName
        case .coinPaymentPurse:
            Self.coinPaymentPurseTypeName
        }
    }

    public var key: String {
        switch self {
        case let .deviceCapability(capability):
            capability.rawValue
        case let .networkAccess(domain):
            domain
        case let .accountAccess(targetProductId):
            targetProductId.rawValue
        case let .nftPurseRead(ownerProductId):
            ownerProductId?.rawValue ?? ""
        case let .coinPaymentPurse(purse, action):
            "\(purse):\(action.persistedName)"
        case .balanceAccess,
             .webRtcAccess,
             .chainSubmitAccess,
             .preimageSubmitAccess,
             .statementSubmitAccess,
             .userIdentityAccess,
             .coinPayment:
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
        case accountAccessTypeName:
            guard let target = try? ProductId(key) else { return nil }
            return .accountAccess(targetProductId: target)
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
        case nftPurseReadTypeName:
            if key.isEmpty { return .nftPurseRead(ownerProductId: nil) }
            guard let owner = try? ProductId(key) else { return nil }
            return .nftPurseRead(ownerProductId: owner)
        case coinPaymentTypeName:
            return .coinPayment
        case coinPaymentPurseTypeName:
            let parts = key.split(separator: ":", omittingEmptySubsequences: false)
            guard parts.count == 2,
                  let purse = UInt32(parts[0]),
                  let action = CoinPaymentPurseAction(persistedName: String(parts[1])) else {
                return nil
            }
            return .coinPaymentPurse(purse: purse, action: action)
        default:
            return nil
        }
    }
}

private extension CoinPaymentPurseAction {
    var persistedName: String {
        switch self {
        case .query: "query"
        case .receive: "receive"
        case .spend: "spend"
        }
    }

    init?(persistedName: String) {
        switch persistedName {
        case "query": self = .query
        case "receive": self = .receive
        case "spend": self = .spend
        default: return nil
        }
    }
}
