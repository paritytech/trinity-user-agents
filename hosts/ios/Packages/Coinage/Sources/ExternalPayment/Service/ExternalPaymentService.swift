import AsyncExtensions
import BigInt
import ExtrinsicService
import Foundation
import FoundationExt
import KeyDerivation
import SDKLogger
import StateMachine
import StructuredConcurrency
import SubstrateSdk
import SubstrateOperation

/// Dependencies needed to construct the external payment processing pipeline.
struct ExternalPaymentDependency {
    let instanceId: CoinageInstanceId
    let coinService: CoinServiceProtocol
    let voucherService: VoucherServiceProtocol
    let assetClassifier: ExternalPaymentAssetClassifier
    let recycler: CoinageRecyclingServicing
    let voucherKeyFactory: any VoucherKeyDeriving
    let voucherMinter: any VoucherMinting
    let recyclerLoader: RecyclerReadinessLoading
    let extrinsicMonitor: ExtrinsicSubmitMonitorFactoryProtocol
    let durability: any CoinageTxServicing
    let originFactory: OriginCreating
    let quotaTracker: any UnloadQuotaTracking
    let blockNumberProvider: BlockInfoProviding
    let dateProvider: any DateProviding
}

/// Manages the lifecycle of external payments.
///
/// Previews payments via the planner, registers them one at a time, and processes non-terminal
/// payments sequentially via the state machine. Every run ends in a persisted verdict: there is no
/// retry and no reschedule — the product gets a terminal answer from one pass.
final class ExternalPaymentService: ExternalPaymentServicing, @unchecked Sendable {
    let store: ExternalPaymentStoring
    let planner: ExternalPaymentPlanning
    let stateMachineFactory: ExternalPaymentStateMachineCreating
    let context: ExternalPaymentContext
    let logger: SDKLoggerProtocol?

    /// Serializes check-then-save so two racing initiations for the same identity cannot both succeed.
    private let registrationQueue = SerialOperationQueue()
    private var observeTask: Task<Void, Never>?

    convenience init(
        store: ExternalPaymentStoring,
        dependency: ExternalPaymentDependency,
        logger: SDKLoggerProtocol? = nil
    ) {
        let planner = ExternalPaymentPlanner(
            coinService: dependency.coinService,
            voucherService: dependency.voucherService,
            classifier: dependency.assetClassifier
        )
        let stateMachineFactory = ExternalPaymentStateMachineFactory(
            instanceId: dependency.instanceId,
            planner: planner,
            voucherService: dependency.voucherService,
            recycler: dependency.recycler,
            voucherKeyFactory: dependency.voucherKeyFactory,
            voucherMinter: dependency.voucherMinter,
            recyclerLoader: dependency.recyclerLoader,
            extrinsicMonitor: dependency.extrinsicMonitor,
            durability: dependency.durability,
            originFactory: dependency.originFactory,
            quotaTracker: dependency.quotaTracker,
            blockNumberProvider: dependency.blockNumberProvider,
            dateProvider: dependency.dateProvider,
            logger: logger
        )

        self.init(store: store, planner: planner, stateMachineFactory: stateMachineFactory, logger: logger)
    }

    init(
        store: ExternalPaymentStoring,
        planner: ExternalPaymentPlanning,
        stateMachineFactory: ExternalPaymentStateMachineCreating,
        logger: SDKLoggerProtocol? = nil
    ) {
        self.store = store
        self.planner = planner
        self.stateMachineFactory = stateMachineFactory
        self.logger = logger
        context = ExternalPaymentContext(logger: logger)
    }

    func previewPayment(
        for amount: Balance,
        context: DenominationBreakdownContext
    ) async throws -> ExternalPaymentPreview {
        try await planner.plan(amount: amount, context: context)
    }

    func canExecuteExternalPaymentPrivately(
        amount: Balance,
        context: DenominationBreakdownContext
    ) async throws -> Bool {
        try await planner.canPayPrivately(amount: amount, context: context)
    }

    func initiatePayment(
        productId: String,
        paymentId: String,
        amountInPlanks: Balance,
        destination: AccountId
    ) async throws {
        guard !paymentId.isEmpty else {
            throw ExternalPaymentError.invalidPaymentId
        }

        let payment = ExternalPayment(
            productId: productId,
            paymentId: paymentId,
            amountInPlanks: amountInPlanks,
            destination: destination
        )

        try await registrationQueue.run { [store] in
            guard try await store.fetchPayment(byId: payment.identifier) == nil else {
                throw ExternalPaymentError.alreadyExists
            }

            try await store.save(payment: payment)
        }
    }

    func subscribePaymentStatus(
        productId: String,
        paymentId: String
    ) -> AnyAsyncSequence<ExternalPaymentStatus> {
        let id = ExternalPayment.identifier(productId: productId, paymentId: paymentId)

        return store.observePayment(id: id)
            .map { payment -> ExternalPaymentStatus in
                guard let payment else { throw ExternalPaymentError.notFound }
                return payment.status
            }
            .removeDuplicates()
            .endAfterTerminal()
    }

    func setup(with context: DenominationBreakdownContext) {
        startObservation(with: context)
    }
}

// MARK: - Processing

private extension ExternalPaymentService {
    func startObservation(with denominationContext: DenominationBreakdownContext) {
        observeTask?.cancel()
        observeTask = Task { [store, context, logger, weak self] in
            do {
                for try await payments in store.observeNonTerminalPayments() {
                    for payment in payments.sorted(by: { $0.createdAt < $1.createdAt }) {
                        await context.scheduleIfNeeded(paymentId: payment.identifier) { [weak self] in
                            Task { [weak self] in
                                await self?.processPayment(
                                    id: payment.identifier,
                                    denominationContext: denominationContext
                                )
                            }
                        }
                    }
                }
            } catch {
                guard !Task.isCancelled else {
                    return
                }

                logger?.error("Observation failed: \(error)")
            }
        }
    }

    /// One machine run per payment; every state persists its own verdict, so nothing is retried here.
    func processPayment(id: String, denominationContext: DenominationBreakdownContext) async {
        logger?.debug("Processing payment \(id)")

        do {
            let machine = try await stateMachineFactory.createStateMachine(
                for: id,
                store: store,
                context: denominationContext
            )
            _ = try await machine.executeUntilTerminal()
        } catch {
            logger?.error("Payment \(id) run failed: \(error)")
        }

        await context.onComplete(paymentId: id)
    }
}

// MARK: - Helpers

private extension ExternalPayment {
    var status: ExternalPaymentStatus {
        switch stage {
        case .plan,
             .onboardCoins,
             .offboardVouchers:
            .processing
        case .completed:
            .completed
        case .partiallyCompleted:
            .partiallyCompleted(settledInPlanks: settledInPlanks)
        case .failed:
            .failed(reason: failureReason ?? "Unknown")
        }
    }
}

private extension ExternalPaymentStatus {
    var isTerminal: Bool {
        switch self {
        case .processing: false
        case .completed,
             .partiallyCompleted,
             .failed: true
        }
    }
}

private extension AsyncSequence where Element == ExternalPaymentStatus, Self: Sendable {
    /// Ends right after the first terminal status. `prefix(while:)` would only end once a *further*
    /// element arrived, which a settled row never produces.
    func endAfterTerminal() -> AnyAsyncSequence<ExternalPaymentStatus> {
        let upstream = self

        return AsyncThrowingStream<ExternalPaymentStatus, Error> { continuation in
            let task = Task {
                do {
                    for try await status in upstream {
                        continuation.yield(status)
                        if status.isTerminal { break }
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        .eraseToAnyAsyncSequence()
    }
}
