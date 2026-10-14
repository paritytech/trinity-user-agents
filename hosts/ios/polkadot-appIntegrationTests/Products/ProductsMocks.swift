@testable import polkadot_app
import Foundation
import KeyDerivation
import Products
import SDKLogger
import AsyncExtensions
import SubstrateSdk
import StatementStore

final class MockChatScriptStorage: ChatScriptStorageProtocol {
    private let scripts: [String: String]

    init(scripts: [String: String]) {
        self.scripts = scripts
    }

    func saveScript(productId _: ProductId, content _: String) throws {}

    func loadScript(productId: ProductId) -> String? {
        scripts[productId]
    }

    func deleteScript(productId _: ProductId) throws {}

    func scriptExists(productId: ProductId) -> Bool {
        scripts[productId] != nil
    }

    func loadData(productId: ProductId, relativePath _: String) -> Data? {
        scripts[productId]?.data(using: .utf8)
    }

    func chatEntrypointRelativePath() -> String {
        "ChatExtension/index.js"
    }
}

final class MockProductsNativeApi: ProductsNativeApiProtocol, @unchecked Sendable {
    enum APIError: Error {
        case timeout
    }

    var sentMessages: [ProductBotMessage] = []
    var accountGetCalls: [ProductAccountId] = []
    var accountGetAliasCalls: [ProductProofContext] = []
    private let localStorage: ProductLocalStorageProtocol

    init(localStorage: ProductLocalStorageProtocol) {
        self.localStorage = localStorage
    }

    func accountGet(_ accountId: ProductAccountId) async throws -> ProductAccountResult {
        accountGetCalls.append(accountId)
        return ProductAccountResult(
            publicKey: "0x0000000000000000000000000000000000000000000000000000000000000000"
        )
    }

    func accountGetAlias(
        context: ProductProofContext,
        ring _: RingLocation
    ) async throws -> AccountGetAliasResult {
        accountGetAliasCalls.append(context)

        return AccountGetAliasResult(
            context: Data([0x00]),
            alias: Data(repeating: 0, count: 32)
        )
    }

    func accountCreateProof(
        context _: ProductProofContext,
        ring _: RingLocation,
        message _: Data
    ) async throws -> RingVrfProofResult {
        RingVrfProofResult(
            proof: Data([0x00]),
            contextualAlias: AccountGetAliasResult(context: Data([0x00]), alias: Data([0x00])),
            ringIndex: 0,
            ringRevision: 0
        )
    }

    func signVrf(_: SignVrfPayload) async throws -> VrfSignature {
        VrfSignature(
            preOutput: Data(repeating: 0, count: 32),
            proof: Data(repeating: 0, count: 64)
        )
    }

    func chainNodes(genesisHash _: String) async throws -> [String] {
        ["wss://asset-hub-paseo-rpc.n.dwellir.com"]
    }

    func chainSupported(genesisHash _: String) async throws -> Bool {
        true
    }

    func sendMessage(_ message: ProductBotMessage, roomId _: String?) async throws -> String {
        sentMessages.append(message)
        return "msg-\(sentMessages.count)"
    }

    func createRoom(_: CreateRoomRequest) async throws -> CreateRoomResult {
        CreateRoomResult(status: .new)
    }

    func subscribeRooms() -> AsyncStream<[RoomInfo]> {
        AsyncStream { $0.finish() }
    }

    func subscribeRooms() async throws -> AsyncExtensions.AnyAsyncSequence<[Products.RoomInfo]> {
        AsyncStream {
            $0.finish()
        }
        .eraseToAnyAsyncSequence()
    }

    func getNonProductAccounts() async throws -> [LegacyAccountResult] {
        [LegacyAccountResult(
            publicKey: "0x0000000000000000000000000000000000000000000000000000000000000000",
            name: "TestAccount"
        )]
    }

    func getUserId() async throws -> GetUserIdResult {
        GetUserIdResult(primaryUsername: "testuser.01")
    }

    func subscribeTheme() async -> AnyAsyncSequence<ProductTheme> {
        AsyncStream {
            $0.yield(ProductTheme(name: "test", variant: .dark))
            $0.finish()
        }
        .eraseToAnyAsyncSequence()
    }

    func signPayload(_: SignTransactionPayload<ProductAccountId>) async throws -> SignResult {
        SignResult(signature: "0x00", signedTx: nil)
    }

    func signPayloadLegacy(_: SignTransactionPayload<SS58Account>) async throws -> SignResult {
        SignResult(signature: "0x00", signedTx: nil)
    }

    func signRaw(_: SigningRawPayload) async throws -> SignResult {
        SignResult(signature: "0x00", signedTx: nil)
    }

    func signRawLegacy(_: SignRawLegacyPayload) async throws -> SignResult {
        SignResult(signature: "0x00", signedTx: nil)
    }

    func localStorageRead(key: String) async throws -> String? {
        await localStorage.read(key: key)
    }

    func localStorageWrite(key: String, value: String) async throws {
        await localStorage.write(key: key, value: value)
    }

    func localStorageClear(key: String) async throws {
        await localStorage.clear(key: key)
    }

    func subscribeLocalStorage(key _: String) -> AnyAsyncSequence<String?> {
        AsyncStream<String?> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func workerBeginOperation(label _: String?) async throws -> UInt32 { 0 }

    func workerEndOperation(id _: UInt32) async throws {}

    func subscribeLocale() -> AnyAsyncSequence<String> {
        AsyncStream<String> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func navigateTo(destination _: String) async throws {}

    func allowNetworkAccess(url _: String) async throws -> Bool {
        true
    }

    func allowWebRtcAccess() async throws -> Bool {
        true
    }

    func lookupPreimage(hash _: Data) async throws -> Data {
        Data()
    }

    func submitPreimage(data _: Data) async throws -> String {
        "0x00"
    }

    func requestDevicePermission(capability _: String) async throws -> Bool {
        true
    }

    func requestRemotePermissions(_: [RemotePermissionRequest]) async throws -> Bool {
        true
    }

    func subscribeStatements(filter _: TopicFilter) throws -> AnyAsyncSequence<Products.StatementsPageDto> {
        AsyncStream {
            $0.finish()
        }
        .eraseToAnyAsyncSequence()
    }

    func createStatementProofAuthorized(_: Products
        .CreateStatementProofAuthorizedDto) async throws -> StatementProofDto {
        fatalError("not implemented in mock")
    }

    func createStatementProof(_: CreateStatementProofDto) async throws -> StatementProofDto {
        fatalError("not implemented in mock")
    }

    func createTransaction(_: Products.CreateTransactionPayload<Products.ProductAccountId>) async throws -> Products
        .CreateTransactionResult {
        fatalError("not implemented in mock")
    }

    func createTransactionLegacy(_: Products
        .CreateTransactionPayload<Products.LegacyAccountId>) async throws -> Products
            .CreateTransactionResult {
        fatalError("not implemented in mock")
    }

    func submitStatement(_: StatementDto) async throws {}

    func subscribePaymentBalance() async throws -> AnyAsyncSequence<PaymentBalance> {
        AsyncStream<PaymentBalance> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func paymentTopUp(amount _: Balance, source _: PaymentTopUpSource, id _: PaymentTopUpId) async throws {}

    func subscribePaymentTopUpStatus(
        id _: PaymentTopUpId
    ) async throws -> AsyncExtensions.AnyAsyncSequence<HostPaymentTopUpStatus> {
        AsyncStream<HostPaymentTopUpStatus> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func requestPayment(amount _: Balance, destination _: AccountId, id _: PaymentRequestId) async throws {}

    func subscribePaymentStatus(id _: PaymentRequestId) async throws -> AsyncExtensions
        .AnyAsyncSequence<HostPaymentStatus> {
        AsyncStream<HostPaymentStatus> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func requestResourceAllocation(resources: [Products.AllocatableResource]) async throws
        -> [Products.AllocationOutcome] {
        resources.map { _ in AllocationOutcome.notAvailable }
    }

    func pushNotification(_: ScheduledNotificationRequest) async throws -> UInt32 { 0 }
    func cancelPushNotification(identifier _: UInt32) async throws {}

    func deriveEntropy(key _: Data) async throws -> Data {
        Data(count: 32)
    }

    /// Polls until the product script sends its first message (e.g. welcome message).
    /// Uses polling instead of continuation to be cancellation-safe.
    func waitForFirstMessage(timeout: TimeInterval = 10) async throws {
        let deadline = Date().addingTimeInterval(timeout)
        while sentMessages.isEmpty {
            guard Date() < deadline else {
                throw APIError.timeout
            }
            try await Task.sleep(for: .milliseconds(100))
        }
    }

    func consumeMessage() -> ProductBotMessage? {
        sentMessages.removeFirst()
    }
}
