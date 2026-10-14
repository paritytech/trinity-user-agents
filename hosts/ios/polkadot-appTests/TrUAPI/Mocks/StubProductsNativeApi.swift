import Foundation
import KeyDerivation
import Products
import AsyncExtensions
import StatementStore
import SubstrateSdk
@testable import polkadot_app

final class StubProductsNativeApi: ProductsNativeApiProtocol {
    /// What `subscribePaymentTopUpStatus` answers: `.failure` throws on subscribe, `.success` replays
    /// the stream. Unset keeps the call unsupported.
    var paymentTopUpStatusResult: Result<AnyAsyncSequence<HostPaymentTopUpStatus>, Error>?

    func accountGet(_: Products.ProductAccountId) async throws -> ProductAccountResult { fatalError() }

    func accountGetAlias(
        context _: Products.ProductProofContext,
        ring _: Products.RingLocation
    ) async throws -> AccountGetAliasResult { fatalError() }

    func accountCreateProof(
        context _: Products.ProductProofContext,
        ring _: Products.RingLocation,
        message _: Data
    ) async throws -> RingVrfProofResult { fatalError() }

    func chainNodes(genesisHash _: String) async throws -> [String] { fatalError() }
    func chainSupported(genesisHash _: String) async throws -> Bool { fatalError() }
    func sendMessage(_: ProductBotMessage, roomId _: String?) async throws -> String { fatalError() }
    func createRoom(_: CreateRoomRequest) async throws -> CreateRoomResult { fatalError() }
    func subscribeRooms() async throws -> AnyAsyncSequence<[RoomInfo]> { fatalError() }
    func getNonProductAccounts() async throws -> [LegacyAccountResult] { fatalError() }
    func signPayload(_: SignTransactionPayload<Products.ProductAccountId>) async throws -> SignResult { fatalError() }
    func signPayloadLegacy(_: SignTransactionPayload<SS58Account>) async throws -> SignResult { fatalError() }
    func signRaw(_: SigningRawPayload) async throws -> SignResult { fatalError() }
    func signVrf(_: SignVrfPayload) async throws -> VrfSignature { fatalError() }
    func signRawLegacy(_: SignRawLegacyPayload) async throws -> SignResult { fatalError() }

    func createTransaction(
        _: CreateTransactionPayload<Products.ProductAccountId>
    ) async throws -> CreateTransactionResult { fatalError() }

    func createTransactionLegacy(
        _: CreateTransactionPayload<LegacyAccountId>
    ) async throws -> CreateTransactionResult { fatalError() }

    func localStorageRead(key _: String) async throws -> String? { fatalError() }
    func localStorageWrite(key _: String, value _: String) async throws { fatalError() }
    func localStorageClear(key _: String) async throws { fatalError() }
    func subscribeLocalStorage(key _: String) -> AnyAsyncSequence<String?> { fatalError() }
    func workerBeginOperation(label _: String?) async throws -> UInt32 { fatalError() }
    func workerEndOperation(id _: UInt32) async throws { fatalError() }
    func navigateTo(destination _: String) async throws { fatalError() }
    func allowNetworkAccess(url _: String) async throws -> Bool { fatalError() }
    func allowWebRtcAccess() async throws -> Bool { fatalError() }
    func lookupPreimage(hash _: Data) async throws -> Data { fatalError() }
    func submitPreimage(data _: Data) async throws -> String { fatalError() }
    func requestDevicePermission(capability _: String) async throws -> Bool { fatalError() }
    func requestRemotePermissions(_: [RemotePermissionRequest]) async throws -> Bool { fatalError() }

    func subscribeStatements(filter _: TopicFilter) throws -> AnyAsyncSequence<StatementsPageDto> {
        fatalError()
    }

    func createStatementProof(_: CreateStatementProofDto) async throws -> StatementProofDto { fatalError() }

    func createStatementProofAuthorized(
        _: CreateStatementProofAuthorizedDto
    ) async throws -> StatementProofDto { fatalError() }

    func submitStatement(_: StatementDto) async throws { fatalError() }
    func subscribePaymentBalance() async throws -> AnyAsyncSequence<PaymentBalance> { fatalError() }

    func requestPayment(
        amount _: Balance,
        destination _: AccountId,
        id _: PaymentRequestId
    ) async throws { fatalError() }

    func subscribePaymentStatus(id _: PaymentRequestId) async throws -> AnyAsyncSequence<HostPaymentStatus> {
        fatalError()
    }

    func paymentTopUp(amount _: Balance, source _: PaymentTopUpSource, id _: PaymentTopUpId) async throws {
        fatalError()
    }

    func subscribePaymentTopUpStatus(
        id _: PaymentTopUpId
    ) async throws -> AnyAsyncSequence<HostPaymentTopUpStatus> {
        guard let paymentTopUpStatusResult else { fatalError() }
        return try paymentTopUpStatusResult.get()
    }

    func pushNotification(_: ScheduledNotificationRequest) async throws -> UInt32 { fatalError() }
    func cancelPushNotification(identifier _: UInt32) async throws { fatalError() }
    func deriveEntropy(key _: Data) async throws -> Data { fatalError() }

    func requestResourceAllocation(
        resources _: [Products.AllocatableResource]
    ) async throws -> [AllocationOutcome] { fatalError() }

    func getUserId() async throws -> GetUserIdResult { fatalError() }
    func subscribeTheme() async -> AnyAsyncSequence<ProductTheme> { fatalError() }
    func subscribeLocale() -> AnyAsyncSequence<String> { fatalError() }
}
