import struct TrUAPIHost.ProductId
import AsyncExtensions
import BigInt
import Foundation
import KeyDerivation
import SubstrateSdk

/// The wallet as the app's screens, chat and legacy product API use it, served by the core's coinage
/// layer. Every operation the app starts is keyed by a ``CoinageOperationId`` it chooses, so a retry
/// joins the operation instead of starting another one.
public protocol CoinageServicing: AnyObject, Sendable {
    /// The conversion between the core's cents and the payment asset's planks.
    func units() async throws -> CoinageUnits

    /// The balance now and after every change, split by the user's privacy strategy.
    func balanceStream() -> AnyAsyncSequence<CoinageBalance>

    /// The balance and the holdings behind it, from one snapshot.
    func summaryStream() -> AnyAsyncSequence<CoinageSummary>

    func balance() async throws -> CoinageBalance

    /// Whether the wallet covers `amount`, and whether that spends value still gaining privacy. Throws
    /// ``TransferPreviewError/insufficientBalance`` when it does not.
    func previewTransfer(for amount: BigUInt) async throws -> TransferPreview

    /// Reserves coins for a payment under `exportId`; a second call with the same id joins the first.
    func prepareTransfer(
        _ preview: TransferPreview,
        exportId: CoinageOperationId,
        retryUntil: Date
    ) async throws -> PreparedTransfer

    /// What became of each coin of the export `exportId`, now and after every change.
    func subscribePaymentStates(exportId: CoinageOperationId) -> AnyAsyncSequence<[CoinagePaymentState]>

    /// Claims the coins `coinKeys` control under `operationId` and follows the claim until it ends.
    func claim(
        coinKeys: [Data],
        operationId: CoinageOperationId,
        retryUntil: Date
    ) -> AnyAsyncSequence<CoinageTransferDetection>

    /// Tops the wallet up with `amount` planks from `externalAssetHolder`'s account and returns once
    /// the top-up has ended.
    func loadVouchers(amount: BigUInt, externalAssetHolder: any WalletManaging) async throws

    /// Starts a product's top-up, or throws ``IncomingPaymentError/alreadyExists`` for an id seen before.
    func acceptTopUp(_ request: CoinageTopUpRequest) async throws

    /// Fails with ``IncomingPaymentError/notFound(_:)`` for an unknown top-up.
    func subscribeTopUpStatus(
        productId: ProductId,
        paymentId: IncomingPaymentId
    ) -> AnyAsyncSequence<IncomingPaymentStatus>

    func previewExternalPayment(for amount: BigUInt) async throws -> ExternalPaymentPreview

    /// Starts paying `amountInPlanks` out to `destination`. Throws ``ExternalPaymentError/alreadyExists``
    /// for an identity seen before. `confirmed` is the user's consent to spend value gaining privacy.
    func initiateExternalPayment(_ request: ExternalPaymentRequest) async throws

    /// Ends after the first terminal status; fails with ``ExternalPaymentError/notFound`` for an
    /// unknown identity.
    func subscribeExternalPaymentStatus(productId: ProductId?, paymentId: String) -> AnyAsyncSequence<ExternalPaymentStatus>

    /// Where the scan of the handed-over wallet and the user's deep searches stand.
    func subscribeBackupProgress() -> AnyAsyncSequence<BackupProgress>

    /// Another look for value under earlier installations of the seed.
    func deepSearchBackup() async

    /// The user accepted the recovered balance.
    func markBackupAsCompleted() async

    /// Whether this installation can be recovered from the seed.
    func subscribeAccountBackupStatus() -> AnyAsyncSequence<CoinageAccountBackupStatus>

    /// Whether the wallet waits for this installation's registration before it builds anything.
    func subscribeAwaitingInstallationBackup() -> AnyAsyncSequence<Bool>

    /// Keeps the core's privacy setting the user's choice until ``stop()``.
    func start()

    func stop()
}

/// A product's top-up: what it pays and where the value comes from.
public struct CoinageTopUpRequest: Sendable {
    public enum Source: Sendable {
        /// An account the app holds the key of, which pays and signs.
        case funding(any WalletManaging)
        /// Coins whose secrets the product handed over, which are claimed.
        case coins(secretKeys: [Data])
    }

    public let productId: ProductId
    public let paymentId: IncomingPaymentId
    public let amount: BigUInt
    public let source: Source

    public init(productId: ProductId, paymentId: IncomingPaymentId, amount: BigUInt, source: Source) {
        self.productId = productId
        self.paymentId = paymentId
        self.amount = amount
        self.source = source
    }
}

/// A payment out of the wallet to an external account.
public struct ExternalPaymentRequest: Sendable {
    /// The product that asked for it, or `nil` for the app's own send screen.
    public let productId: ProductId?
    public let paymentId: String
    public let amountInPlanks: BigUInt
    public let destination: AccountId
    /// The user agreed to spend value still gaining privacy.
    public let confirmed: Bool

    public init(
        productId: ProductId?,
        paymentId: String,
        amountInPlanks: BigUInt,
        destination: AccountId,
        confirmed: Bool
    ) {
        self.productId = productId
        self.paymentId = paymentId
        self.amountInPlanks = amountInPlanks
        self.destination = destination
        self.confirmed = confirmed
    }
}
