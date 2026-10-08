import Foundation
import KeyDerivation
import SubstrateSdk
import AsyncExtensions
import StatementStore

/// Native capabilities exposed to product scripts via the container bridge.
public protocol ProductsNativeApiProtocol: AnyObject {
    func accountGet(_ accountId: ProductAccountId) async throws -> ProductAccountResult

    func accountGetAlias(
        context: ProductProofContext,
        ring: RingLocation
    ) async throws -> AccountGetAliasResult

    func accountCreateProof(
        context: ProductProofContext,
        ring: RingLocation,
        message: Data
    ) async throws -> RingVrfProofResult

    func signVrf(_ payload: SignVrfPayload) async throws -> VrfSignature
    func chainNodes(genesisHash: String) async throws -> [String]
    func chainSupported(genesisHash: String) async throws -> Bool
    func sendMessage(_ message: ProductBotMessage, roomId: String?) async throws -> String
    func createRoom(_ request: CreateRoomRequest) async throws -> CreateRoomResult
    func subscribeRooms() async throws -> AnyAsyncSequence<[RoomInfo]>
    func getNonProductAccounts() async throws -> [LegacyAccountResult]
    func signPayload(_ payload: SignTransactionPayload<ProductAccountId>) async throws -> SignResult
    func signPayloadLegacy(_ payload: SignTransactionPayload<SS58Account>) async throws -> SignResult
    func signRaw(_ payload: SigningRawPayload) async throws -> SignResult
    func signRawLegacy(_ payload: SignRawLegacyPayload) async throws -> SignResult
    func createTransaction(_ payload: CreateTransactionPayload<ProductAccountId>) async throws
        -> CreateTransactionResult
    func createTransactionLegacy(_ payload: CreateTransactionPayload<LegacyAccountId>) async throws
        -> CreateTransactionResult
    func localStorageRead(key: String) async throws -> String?
    func localStorageWrite(key: String, value: String) async throws
    func localStorageClear(key: String) async throws
    func subscribeLocalStorage(key: String) -> AnyAsyncSequence<String?>

    /// Opens a keep-alive operation for the calling product; returns its host-assigned id.
    func workerBeginOperation(label: String?) async throws -> UInt32
    /// Closes a keep-alive operation. Idempotent: an unknown or already-ended id still succeeds.
    func workerEndOperation(id: UInt32) async throws
    func navigateTo(destination: String) async throws
    func allowNetworkAccess(url: String) async throws -> Bool
    func allowWebRtcAccess() async throws -> Bool

    // Preimage
    func lookupPreimage(hash: Data) async throws -> Data
    func submitPreimage(data: Data) async throws -> String

    // Permissions
    func requestDevicePermission(capability: String) async throws -> Bool
    func requestRemotePermissions(_ requests: [RemotePermissionRequest]) async throws -> Bool

    // Statement Store
    func subscribeStatements(filter: TopicFilter) throws -> AnyAsyncSequence<StatementsPageDto>

    func createStatementProof(_ request: CreateStatementProofDto) async throws -> StatementProofDto

    func createStatementProofAuthorized(
        _ request: CreateStatementProofAuthorizedDto
    ) async throws -> StatementProofDto

    func submitStatement(_ statement: StatementDto) async throws

    // Payments
    func subscribePaymentBalance() async throws -> AnyAsyncSequence<PaymentBalance>
    func requestPayment(amount: Balance, destination: AccountId, id: PaymentRequestId) async throws
    func subscribePaymentStatus(id: PaymentRequestId) async throws -> AnyAsyncSequence<HostPaymentStatus>

    func paymentTopUp(amount: Balance, source: PaymentTopUpSource, id: PaymentTopUpId) async throws

    func subscribePaymentTopUpStatus(id: PaymentTopUpId) async throws -> AnyAsyncSequence<HostPaymentTopUpStatus>

    // Push Notification
    func pushNotification(_ request: ScheduledNotificationRequest) async throws -> UInt32
    func cancelPushNotification(identifier: UInt32) async throws

    // Entropy Derivation
    func deriveEntropy(key: Data) async throws -> Data

    // Resource Allocation
    func requestResourceAllocation(
        resources: [AllocatableResource]
    ) async throws -> [AllocationOutcome]

    func getUserId() async throws -> GetUserIdResult

    // Theme
    func subscribeTheme() async -> AnyAsyncSequence<ProductTheme>

    /// Emits the language the host presents its interface in as a BCP 47 tag,
    /// once on subscribe and again on every change.
    func subscribeLocale() -> AnyAsyncSequence<String>
}

// MARK: - Account

public struct ProductAccountResult {
    public let publicKey: String

    public init(publicKey: String) {
        self.publicKey = publicKey
    }
}

public struct LegacyAccountResult {
    public let publicKey: String
    public let name: String?

    public init(publicKey: String, name: String?) {
        self.publicKey = publicKey
        self.name = name
    }
}

public struct GetUserIdResult {
    public let primaryUsername: String

    public init(primaryUsername: String) {
        self.primaryUsername = primaryUsername
    }
}

// MARK: - Theme

public struct ProductTheme: Equatable {
    public enum Variant: String {
        case light = "Light"
        case dark = "Dark"
    }

    public let name: String
    public let variant: Variant

    public init(name: String, variant: Variant) {
        self.name = name
        self.variant = variant
    }
}

public struct AccountGetAliasResult: Codable {
    @HexCodable public var context: Data
    @HexCodable public var alias: Data

    public init(context: Data, alias: Data) {
        _context = HexCodable(wrappedValue: context)
        _alias = HexCodable(wrappedValue: alias)
    }
}

/// Wire form of ``RingVrfProof`` returned to product scripts; bytes encode as hex.
public struct RingVrfProofResult: Codable {
    @HexCodable public var proof: Data
    public let contextualAlias: AccountGetAliasResult
    public let ringIndex: UInt32
    public let ringRevision: UInt32

    public init(
        proof: Data,
        contextualAlias: AccountGetAliasResult,
        ringIndex: UInt32,
        ringRevision: UInt32
    ) {
        _proof = HexCodable(wrappedValue: proof)
        self.contextualAlias = contextualAlias
        self.ringIndex = ringIndex
        self.ringRevision = ringRevision
    }
}

// MARK: - Messages

public enum ProductBotMessage {
    case text(String)
    /// `alt` is the product's one-line description, shown where the card is listed rather than drawn.
    case custom(messageType: String, data: Data, alt: String?)
}

// MARK: - Signing

public struct SigningRawPayload {
    public let account: ProductAccountId
    public let content: RawPayloadContent

    public init(account: ProductAccountId, content: RawPayloadContent) {
        self.account = account
        self.content = content
    }
}

public struct SignRawLegacyPayload {
    public let account: AccountId
    public let content: RawPayloadContent

    public init(account: AccountId, content: RawPayloadContent) {
        self.account = account
        self.content = content
    }
}

public enum RawPayloadContent {
    case bytes(Data)
    case payload(String)
}

public struct SignResult {
    public let signature: String
    public let signedTx: String?

    public init(signature: String, signedTx: String?) {
        self.signature = signature
        self.signedTx = signedTx
    }
}
