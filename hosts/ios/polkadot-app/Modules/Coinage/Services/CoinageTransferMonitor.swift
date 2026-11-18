import AsyncAlgorithms
import BigInt
import Coinage
import CommonService
import Foundation
import Operation_iOS
import SubstrateSdk

/// Persists each chat transfer's lifecycle as its state row (``TransferStateStoring``). The status
/// comes only from the durability-derived streams: a thrown runtime error is never written, and a
/// message whose row is terminal never enters the subscriptions.
protocol CoinageTransferMonitoring: AsyncApplicationServicing {}

final class CoinageTransferMonitor {
    private let coinageService: any CoinageServicing
    private let transferStateStore: any TransferStateStoring
    private let messageProviderFactory: ChatMessageDataProviderMaking
    private let logger: LoggerProtocol

    /// Top-level tasks that listen to the CoreData message streams.
    private var incomingTransfersSubscription: Task<Void, Never>?
    private var outgoingTransfersSubscription: Task<Void, Never>?

    /// Per-message tasks keyed by messageId. Each resolves independently, avoiding head-of-line
    /// blocking across messages.
    private let taskRegistry = ActiveTaskRegistry()

    init(
        coinageService: any CoinageServicing,
        transferStateStore: any TransferStateStoring,
        storageFacade: StorageFacadeProtocol,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.coinageService = coinageService
        self.transferStateStore = transferStateStore
        self.logger = logger

        let repositoryFactory = ChatMessageRepositoryFactory(storageFacade: storageFacade)
        messageProviderFactory = ChatMessageDataProviderFactory(
            repositoryFactory: repositoryFactory,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}

extension CoinageTransferMonitor: CoinageTransferMonitoring {
    func setup() async {
        subscribeIncomingMessages()
        subscribeOutgoingMessages()
    }

    func throttle() async {
        incomingTransfersSubscription?.cancel()
        outgoingTransfersSubscription?.cancel()
        await taskRegistry.cancelAll()
    }
}

// MARK: - Incoming (claim)

private extension CoinageTransferMonitor {
    func subscribeIncomingMessages() {
        logger.debug("Going to subscribe to incoming messages")

        incomingTransfersSubscription = Task { [weak self] in
            guard let self else { return }
            do {
                let stream = messageProviderFactory.subscribeMessages(with: .incomingCoinageSendMessages())
                for try await messages in stream {
                    try Task.checkCancellation()

                    logger.debug("Found \(messages.count) incoming coinage messages")

                    for message in messages {
                        await startIncoming(for: message)
                    }
                }
            } catch {
                guard !Task.isCancelled else { return }
                logger.error("Coinage claim subscription failed: \(error)")
            }
        }
    }

    func startIncoming(for message: Chat.LocalMessage) async {
        guard case let .coinageSend(content) = message.content else { return }
        let messageId = message.messageId
        guard await taskRegistry.contains(messageId) == false else { return }

        let coinKeys = content.coinKeys

        let task = Task { [coinageService, transferStateStore, taskRegistry, logger] in
            defer { Task { await taskRegistry.remove(forMessageId: messageId) } }
            do {
                // Anchored to this device's first attempt so two devices' clocks never decide the window.
                let claimAnchor = try await transferStateStore.beginIncoming(messageId: messageId)
                let retryUntil = claimAnchor.addingTimeInterval(CoinageConstants.claimRetryWindow)

                logger.debug("Starting processing incoming coinage message=\(messageId)")

                let states = try coinageService.claim(
                    coinKeys: coinKeys,
                    operationId: .chatClaim(messageId: messageId),
                    retryUntil: retryUntil
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
    func subscribeOutgoingMessages() {
        logger.debug("Going to subscribe to outgoing messages")

        outgoingTransfersSubscription = Task { [weak self] in
            guard let self else { return }
            do {
                let stream = messageProviderFactory.subscribeMessages(with: .outgoingLocalDeviceCoinageSendMessages())
                for try await messages in stream {
                    try Task.checkCancellation()
                    for message in messages {
                        await startOutgoing(for: message)
                    }
                }
            } catch {
                guard !Task.isCancelled else { return }
                logger.error("Coinage send subscription failed: \(error)")
            }
        }
    }

    func startOutgoing(for message: Chat.LocalMessage) async {
        guard case let .coinageSend(content) = message.content else { return }
        let messageId = message.messageId
        guard await taskRegistry.contains(messageId) == false else { return }

        let task = Task { [coinageService, transferStateStore, taskRegistry, logger] in
            defer { Task { await taskRegistry.remove(forMessageId: messageId) } }
            do {
                logger.debug("Processing transfer for message: \(messageId) entries=\(content.coinKeys.count)")

                let exportId = try CoinageOperationId.chatSend(messageId: messageId)
                var lastState: OutgoingTransferState?
                for try await coinStates in coinageService.subscribePaymentStates(exportId: exportId) {
                    let state = coinStates.outgoingState
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

extension CoinageTransferDetection {
    /// `claimingRest` is plain `claiming`: the claimed-so-far amount is deliberately not persisted.
    var incomingState: IncomingTransferState {
        switch self {
        case .detecting:
            IncomingTransferState(status: .detecting)
        case .claiming,
             .claimingRest:
            IncomingTransferState(status: .claiming)
        case let .claimed(amount, _):
            IncomingTransferState(status: .claimed, actualValue: amount)
        case let .claimedPartially(claimed):
            IncomingTransferState(status: .claimed, actualValue: claimed)
        case .notClaimed:
            IncomingTransferState(status: .failed)
        }
    }
}

extension [CoinagePaymentState] {
    /// Any coin still to be taken keeps the message at `sending`/`sent`; once nothing is outstanding,
    /// the claimed value is final.
    var outgoingState: OutgoingTransferState {
        guard !isEmpty else { return OutgoingTransferState(status: .sending) }

        let claimed = filter { if case .claimed = $0.status { true } else { false } }
        let awaiting = filter { $0.status == .awaitingClaim }
        let outstanding = filter { $0.status == .awaitingClaim || $0.status == .detecting }

        if !outstanding.isEmpty {
            return OutgoingTransferState(status: awaiting.isEmpty && claimed.isEmpty ? .sending : .sent)
        }
        guard !claimed.isEmpty else { return OutgoingTransferState(status: .failed) }

        return OutgoingTransferState(status: .claimed, actualValue: claimed.reduce(Balance(0)) { $0 + $1.value })
    }

    var isSettled: Bool {
        !isEmpty && allSatisfy(\.status.isTerminal)
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
