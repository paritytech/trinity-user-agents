import AsyncAlgorithms
import BigInt
import Coinage
import CommonService
import Foundation
import Operation_iOS
import SubstrateSdk

/// Persists each chat payment's lifecycle as its state (``TransferStateStoring``). The status
/// comes only from the durability-derived streams: a thrown runtime error is never written, and a
/// payment whose state is terminal never enters the subscription.
protocol CoinageTransferMonitoring: AsyncApplicationServicing {}

final class CoinageTransferMonitor {
    typealias DenominationContextProvider = @Sendable () async throws -> DenominationBreakdownContext

    private let claimCoinsService: any ClaimCoinsServicing
    private let transferStatusService: any CoinageTransferStatusServicing
    private let denominationContext: DenominationContextProvider
    /// The payments of the rooms of the chat product, the only chat that carries payments.
    private let productPayments: any ProductChatPaymentsMonitoring
    private let logger: LoggerProtocol

    /// The top-level task that listens to the unsettled payments.
    private var productPaymentsSubscription: Task<Void, Never>?

    /// Per-message tasks keyed by messageId. Each resolves independently, avoiding head-of-line
    /// blocking across messages.
    private let taskRegistry = ActiveTaskRegistry()

    init(
        claimCoinsService: any ClaimCoinsServicing,
        transferStatusService: any CoinageTransferStatusServicing,
        denominationContext: @escaping DenominationContextProvider,
        productPayments: any ProductChatPaymentsMonitoring,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.claimCoinsService = claimCoinsService
        self.transferStatusService = transferStatusService
        self.denominationContext = denominationContext
        self.productPayments = productPayments
        self.logger = logger
    }
}

extension CoinageTransferMonitor: CoinageTransferMonitoring {
    func setup() async {
        subscribeProductPayments()
    }

    func throttle() async {
        productPaymentsSubscription?.cancel()
        await taskRegistry.cancelAll()
    }
}

// MARK: - Incoming (claim)

private extension CoinageTransferMonitor {
    func startIncoming(
        messageId: Chat.MessageId,
        memo: TransferMemo,
        stateStore transferStateStore: any TransferStateStoring
    ) async {
        guard await taskRegistry.contains(messageId) == false else { return }

        let task = Task { [claimCoinsService, denominationContext, taskRegistry, logger] in
            defer { Task { await taskRegistry.remove(forMessageId: messageId) } }
            do {
                let context = try await denominationContext()

                // Anchored to this device's first attempt so two devices' clocks never decide the window.
                let retryUntil = try await transferStateStore
                    .beginIncoming(messageId: messageId)
                    .addingTimeInterval(CoinageConstants.claimRetryWindow)

                logger.debug("Starting processing incoming coinage message=\(messageId)")

                let states = claimCoinsService.claim(
                    coinKeys: memo.entries,
                    groupId: messageId,
                    retryUntil: retryUntil,
                    context: context
                )
                .map(\.incomingState)
                .removeDuplicates()

                for try await state in states {
                    try await transferStateStore.updateIncoming(messageId: messageId, state: state)
                }
            } catch {
                // Not a status: the message is retried on the next snapshot or launch.
                logger.error("Failed to claim coinage for \(messageId): \(error)")
            }
        }
        await taskRegistry.register(task, forMessageId: messageId)
    }
}

// MARK: - Outgoing (payment status)

private extension CoinageTransferMonitor {
    func startOutgoing(
        messageId: Chat.MessageId,
        memo: TransferMemo,
        stateStore transferStateStore: any TransferStateStoring
    ) async {
        guard await taskRegistry.contains(messageId) == false else { return }

        let task = Task { [transferStatusService, denominationContext, taskRegistry, logger] in
            defer { Task { await taskRegistry.remove(forMessageId: messageId) } }
            do {
                let context = try await denominationContext()

                logger.debug("Processing transfer for message: \(messageId) entries=\(memo.entries.count)")

                // Deduplicated by hand: the context is not Sendable, so it cannot ride a `map`.
                var lastState: OutgoingTransferState?
                for try await coinStates in transferStatusService.subscribeStatuses(coinKeys: memo.entries) {
                    let state = coinStates.outgoingState(context: context)
                    if state != lastState {
                        lastState = state
                        logger.debug("Status=\(state) message=\(messageId)")
                        try await transferStateStore.updateOutgoing(messageId: messageId, state: state)
                    }
                    if coinStates.isSettled { break }
                }
            } catch {
                logger.error("Send status monitoring failed for \(messageId): \(error)")
            }
        }
        await taskRegistry.register(task, forMessageId: messageId)
    }
}

// MARK: - Payments of the chat product

private extension CoinageTransferMonitor {
    /// Each payment is claimed or followed under its message id, which is also the group of its coins.
    func subscribeProductPayments() {
        logger.debug("Going to subscribe to chat product payments")

        productPaymentsSubscription = Task { [weak self] in
            guard let productPayments = self?.productPayments else { return }
            for await payments in await productPayments.unsettledPayments() {
                guard let self, !Task.isCancelled else { return }

                for payment in payments {
                    if payment.incoming {
                        await startIncoming(
                            messageId: payment.messageId,
                            memo: payment.memo,
                            stateStore: productPayments.transferStates
                        )
                    } else {
                        await startOutgoing(
                            messageId: payment.messageId,
                            memo: payment.memo,
                            stateStore: productPayments.transferStates
                        )
                    }
                }
            }
        }
    }
}

// MARK: - Active Task Registry

private actor ActiveTaskRegistry {
    private var tasks: [String: Task<Void, Never>] = [:]

    func register(_ task: Task<Void, Never>, forMessageId id: String) {
        tasks[id] = task
    }

    func remove(forMessageId id: String) {
        tasks.removeValue(forKey: id)
    }

    func contains(_ id: String) -> Bool {
        tasks[id] != nil
    }

    func cancelAll() {
        tasks.values.forEach { $0.cancel() }
        tasks.removeAll()
    }
}
