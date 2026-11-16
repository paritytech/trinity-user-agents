import Foundation
import BigInt
import Coinage
import Products
import TrUAPIHost

/// Serves the Rust core's top-up, payment and balance platforms from the engines the JS bridge
/// uses, through the same ``ProductPayments`` steps. Installed once on the shared runtime.
///
/// The core checks the session, the source keys and balance access itself, so none of that is
/// repeated here, and a short balance is always `InsufficientBalance`: the core turns it into
/// `Rejected` for a product without balance access. Amounts arrive as decimal strings, so they are
/// still parsed here, and one that does not parse to a positive number is `InvalidAmount`.
///
/// The core decides which ids and when the balance to follow, and keeps the latest status of each.
/// A follow call streams the engine's statuses to the runtime until a final one or the stream's end.
final class RustPaymentsBridge: TopUpHostBridge, PaymentHostBridge, BalanceHostBridge, @unchecked Sendable {
    private let payments: ProductPayments
    private let logger: LoggerProtocol
    private let target: RuntimeTarget

    init(payments: ProductPayments, logger: LoggerProtocol) {
        self.payments = payments
        self.logger = logger
        target = RuntimeTarget()
    }

    /// Serve `runtime`'s payment platforms.
    func install(on runtime: TrUAPIHostRuntime) {
        target.runtime = runtime

        runtime.setTopUp(self)
        runtime.setPayments(self)
        runtime.setBalance(self)
    }

    // MARK: - TopUpHostBridge

    func topUp(productId: ProductId, request: HostPaymentTopUpRequest) async throws {
        guard request.into == nil else {
            throw TrUAPIHostPaymentTopUpError.Unknown(reason: Self.onlyMainPurse)
        }
        guard let amount = BigUInt(request.amount), amount > 0 else {
            throw TrUAPIHostPaymentTopUpError.InvalidAmount
        }
        guard let source = try? request.source.toAppSource() else {
            throw TrUAPIHostPaymentTopUpError.InvalidSource
        }

        do {
            try await payments.acceptTopUp(productId: productId, id: request.id, amount: amount, source: source)
        } catch {
            logger.error("[truapi] top-up could not be registered: \(error)")
            throw TrUAPIHostPaymentTopUpError(error, unknownReason: Self.topUpRegistrationFailed)
        }
    }

    func followTopUp(productId: ProductId, id: Data) async throws {
        do {
            for try await status in try await payments.topUpStatuses(productId: productId, id: id) {
                try target.runtime?.notifyTopUpStatus(productId: productId, id: id, status: status.wireStatus)
                if status.isTerminal { return }
            }
        } catch IncomingPaymentError.notFound {
            throw HostPaymentTopUpStatusSubscribeError.TopUpNotFound
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            logger.error("[truapi] top-up status stream failed: \(error)")
            throw HostPaymentTopUpStatusSubscribeError.Unknown(reason: Self.statusUnavailable)
        }
    }

    // MARK: - PaymentHostBridge

    func requestPayment(productId: ProductId, request: HostPaymentRequest) async throws {
        guard request.from == nil else {
            throw HostPaymentError.Unknown(reason: Self.onlyMainPurse)
        }
        guard let amount = BigUInt(request.amount), amount > 0 else {
            throw HostPaymentError.InvalidAmount
        }

        do {
            guard try await payments.canSpend(amount) else {
                throw HostPaymentError.InsufficientBalance
            }
            guard try await payments.awaitUserConsent(
                productId: productId,
                amount: amount,
                destination: request.destination
            ) else {
                throw HostPaymentError.Rejected
            }
            try await payments.initiatePayment(
                productId: productId,
                id: request.id,
                amount: amount,
                destination: request.destination
            )
        } catch let error as HostPaymentError {
            throw error
        } catch ExternalPaymentError.alreadyExists {
            throw HostPaymentError.AlreadyExists
        } catch {
            logger.error("[truapi] payment could not be started: \(error)")
            throw HostPaymentError.Unknown(reason: Self.paymentFailed)
        }
    }

    func followPayment(productId: ProductId, id: Data) async throws {
        do {
            for try await status in payments.paymentStatuses(productId: productId, id: id) {
                let wire = status.wireStatus
                try target.runtime?.notifyPaymentStatus(productId: productId, id: id, status: wire)
                if wire.isFinal { return }
            }
        } catch ExternalPaymentError.notFound {
            throw HostPaymentStatusSubscribeError.PaymentNotFound
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            logger.error("[truapi] payment status stream failed: \(error)")
            throw HostPaymentStatusSubscribeError.Unknown(reason: Self.statusUnavailable)
        }
    }

    // MARK: - BalanceHostBridge

    func followBalance() async throws {
        do {
            for try await balance in try await payments.balanceStream() {
                target.runtime?.notifyBalance(available: String(balance.spendableByPayment))
            }
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            logger.warning("[truapi] balance stream failed: \(error)")
            throw HostRejection.Rejected(reason: Self.balanceUnavailable)
        }
    }
}

// MARK: - Wire reasons

private extension RustPaymentsBridge {
    /// What a product is told on an unclassified failure. The real error is logged; a CoreData or
    /// Keychain dump is not for product scripts.
    static let topUpRegistrationFailed = "top-up could not be registered"
    static let paymentFailed = "payment could not be started"
    static let balanceUnavailable = "balance is unavailable"
    static let onlyMainPurse = "only the main purse is supported"
    static let statusUnavailable = "status is unavailable"
    static let paymentDidNotComplete = "payment did not complete"
}

// MARK: - Runtime target

/// The runtime statuses are pushed to, held weakly: the runtime retains this bridge.
private final class RuntimeTarget: @unchecked Sendable {
    private let lock = NSLock()
    private weak var storedRuntime: TrUAPIHostRuntime?

    var runtime: TrUAPIHostRuntime? {
        get {
            lock.lock()
            defer { lock.unlock() }
            return storedRuntime
        }
        set {
            lock.lock()
            storedRuntime = newValue
            lock.unlock()
        }
    }
}

// MARK: - Wire mapping

private extension TrUAPIHostPaymentTopUpSource {
    func toAppSource() throws -> Products.PaymentTopUpSource {
        switch self {
        case let .productAccount(derivationIndex):
            try .productAccount(derivationIndex: derivationIndex.toSelector())
        case let .privateKey(secretKey):
            .privateKey(secretKey)
        case let .coins(secretKeys):
            .coins(secretKeys: secretKeys)
        }
    }
}

private extension TrUAPIHostPaymentTopUpError {
    /// The coded error for `error`; anything that is not a classified `IncomingPaymentError` becomes
    /// `Unknown` with the generic `unknownReason` rather than the error's own description.
    init(_ error: any Error, unknownReason: String) {
        if error is ProductTopUpSourceError {
            self = .InvalidSource
            return
        }
        switch error as? IncomingPaymentError {
        case .alreadyExists: self = .AlreadyExists
        case .invalidSource: self = .InvalidSource
        case .sourceBusy: self = .SourceBusy
        case .invalidAmount: self = .InvalidAmount
        case .notFound,
             .unknown,
             .none: self = .Unknown(reason: unknownReason)
        }
    }
}

private extension IncomingPaymentStatus {
    var wireStatus: HostPaymentTopUpStatusSubscribeItem {
        switch self {
        case .detecting: .detecting
        case .claiming: .claiming
        case let .claimed(finalized): .claimed(finalized: finalized)
        case let .claimedPartially(actualClaimed): .claimedPartially(actualClaimed: String(actualClaimed))
        case .notClaimed: .notClaimed
        }
    }
}

private extension ExternalPaymentStatus {
    var wireStatus: HostPaymentStatusSubscribeItem {
        switch self {
        case .processing: .processing
        case .completed: .completed
        case let .partiallyCompleted(settled): .claimedPartially(actualClaimed: String(settled))
        // The engine's reason can name a shortfall the core masks; products get a fixed one.
        case .failed: .failed(reason: RustPaymentsBridge.paymentDidNotComplete)
        }
    }
}

private extension HostPaymentStatusSubscribeItem {
    /// `Completed`, `Failed` and `ClaimedPartially` do not change again.
    var isFinal: Bool {
        switch self {
        case .processing: false
        case .completed,
             .failed,
             .claimedPartially: true
        }
    }
}
