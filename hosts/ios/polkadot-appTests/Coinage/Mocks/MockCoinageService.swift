import struct TrUAPIHost.ProductId
import AsyncExtensions
import BigInt
import Coinage
import Foundation
import KeyDerivation

/// Replays scripted claim detections and export states, and records what was asked. Everything else
/// the screens and chat do not reach in these tests fails with ``Unscripted``.
final class MockCoinageService: CoinageServicing, @unchecked Sendable {
    struct ClaimCall: Equatable {
        let coinKeys: [Data]
        let operationId: CoinageOperationId
        let retryUntil: Date
    }

    struct Unscripted: Error {}

    private let lock = NSLock()
    private var recordedClaims: [ClaimCall] = []
    private var recordedExports: [CoinageOperationId] = []

    let detections: [CoinageTransferDetection]
    let claimFailure: Error?
    let paymentSnapshots: [[CoinagePaymentState]]

    init(
        detections: [CoinageTransferDetection] = [],
        claimFailure: Error? = nil,
        paymentSnapshots: [[CoinagePaymentState]] = []
    ) {
        self.detections = detections
        self.claimFailure = claimFailure
        self.paymentSnapshots = paymentSnapshots
    }

    var claimCalls: [ClaimCall] {
        lock.lock()
        defer { lock.unlock() }
        return recordedClaims
    }

    var paymentSubscriptions: [CoinageOperationId] {
        lock.lock()
        defer { lock.unlock() }
        return recordedExports
    }

    func claim(
        coinKeys: [Data],
        operationId: CoinageOperationId,
        retryUntil: Date
    ) -> AnyAsyncSequence<CoinageTransferDetection> {
        lock.lock()
        recordedClaims.append(ClaimCall(coinKeys: coinKeys, operationId: operationId, retryUntil: retryUntil))
        lock.unlock()

        return Self.replay(detections, failure: claimFailure)
    }

    func subscribePaymentStates(exportId: CoinageOperationId) -> AnyAsyncSequence<[CoinagePaymentState]> {
        lock.lock()
        recordedExports.append(exportId)
        lock.unlock()

        return Self.replay(paymentSnapshots, failure: nil)
    }

    func units() async throws -> CoinageUnits { throw Unscripted() }

    func balanceStream() -> AnyAsyncSequence<CoinageBalance> { Self.replay([], failure: Unscripted()) }

    func summaryStream() -> AnyAsyncSequence<CoinageSummary> { Self.replay([], failure: Unscripted()) }

    func balance() async throws -> CoinageBalance { throw Unscripted() }

    func previewTransfer(for _: BigUInt) async throws -> TransferPreview { throw Unscripted() }

    func prepareTransfer(
        _: TransferPreview,
        exportId _: CoinageOperationId,
        retryUntil _: Date
    ) async throws -> PreparedTransfer {
        throw Unscripted()
    }

    func loadVouchers(amount _: BigUInt, externalAssetHolder _: any WalletManaging) async throws {
        throw Unscripted()
    }

    func acceptTopUp(_: CoinageTopUpRequest) async throws { throw Unscripted() }

    func subscribeTopUpStatus(
        productId _: ProductId,
        paymentId _: IncomingPaymentId
    ) -> AnyAsyncSequence<IncomingPaymentStatus> {
        Self.replay([], failure: Unscripted())
    }

    func previewExternalPayment(for _: BigUInt) async throws -> ExternalPaymentPreview { throw Unscripted() }

    func initiateExternalPayment(_: ExternalPaymentRequest) async throws { throw Unscripted() }

    func subscribeExternalPaymentStatus(
        productId _: ProductId?,
        paymentId _: String
    ) -> AnyAsyncSequence<ExternalPaymentStatus> {
        Self.replay([], failure: Unscripted())
    }

    func subscribeBackupProgress() -> AnyAsyncSequence<BackupProgress> { Self.replay([], failure: Unscripted()) }

    func deepSearchBackup() async {}

    func markBackupAsCompleted() async {}

    func subscribeAccountBackupStatus() -> AnyAsyncSequence<CoinageAccountBackupStatus> {
        Self.replay([], failure: Unscripted())
    }

    func subscribeAwaitingInstallationBackup() -> AnyAsyncSequence<Bool> { Self.replay([], failure: Unscripted()) }

    func start() {}

    func stop() {}
}

private extension MockCoinageService {
    static func replay<Element>(_ elements: [Element], failure: Error?) -> AnyAsyncSequence<Element> {
        AsyncThrowingStream<Element, Error> { continuation in
            for element in elements {
                continuation.yield(element)
            }
            if let failure {
                continuation.finish(throwing: failure)
            } else {
                continuation.finish()
            }
        }
        .eraseToAnyAsyncSequence()
    }
}
