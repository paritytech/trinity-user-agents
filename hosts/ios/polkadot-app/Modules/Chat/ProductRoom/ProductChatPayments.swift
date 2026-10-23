import BigInt
import Coinage
import DurableTransactions
import Foundation
import TrUAPIHost

/// Gives a payment of the user to the product of its room (`SendPayment`).
protocol ProductPaymentDelivering: Sendable {
    /// The message that the product made, or nil when it only acknowledged the payment. Throws
    /// ``ProductOutboxRefusal`` when the product refused it.
    func deliver(_ payment: ProductChatPayment) async throws -> ChatMessage?
}

/// The payments of the rooms of the chat product, which the transfer monitor follows and claims as it does the
/// payments of the native chat.
protocol ProductChatPaymentsMonitoring: Sendable {
    /// The payments that the monitor still has work for, now and after each change.
    func unsettledPayments() async -> AsyncStream<[ProductChatPayment]>
    /// Where the monitor writes the state of each payment.
    var transferStates: any TransferStateStoring { get }
}

/// The payments of the user and of the contacts in the rooms of the chat product.
///
/// The coins stay with the host, as in the native chat. A payment of the user is kept with the handoff of its coins
/// in one transaction, and then goes to the product as `SendPayment` under the same message id until the product has
/// it: a crash or a failed send sends the same payment again, which the product answers with the same message. A
/// payment of a contact arrives through `payment_received`, and the transfer monitor claims its coins. The rooms draw
/// the native transfer bubble with the state that the monitor writes here.
actor ProductChatPayments {
    struct Settings: Sendable {
        /// The wait before the first new send of a payment that did not go. Each next wait doubles.
        var firstRetryDelay: Duration = .seconds(2)
        var maxRetryDelay: Duration = .seconds(60)
    }

    /// What happened to a payment, for the rooms.
    enum Event: Equatable, Sendable {
        /// The host keeps a new payment of the user, which goes to the product now.
        case staged(ProductChatPayment)
        /// The product has the payment of the user. `created` is the message that the product made.
        case delivered(ProductChatPayment, created: ChatMessage?)
        case refused(ProductChatPayment)
        /// The transfer of a payment has a new state.
        case state(messageId: Chat.MessageId, Chat.LocalMessage.Content.Transfer.State)
    }

    fileprivate enum Outcome {
        case delivered
        case refused
        case retry
    }

    static let shared = ProductChatPayments(
        store: ProductChatPaymentStore(storageFacade: UserDataStorageFacade.shared),
        delivery: ProductOutboxDelivery.shared,
        worker: TrUAPIProductHopUploads()
    )

    private let store: any ProductChatPaymentStoring
    private let delivery: any ProductPaymentDelivering
    /// Holds the worker of the product while a payment goes, and tells when it starts again.
    private let worker: any ProductHopUploading
    private let settings: Settings
    private let logger: LoggerProtocol

    private var runner: Task<Void, Never>?
    private var runnerId: UUID?
    private var isWaiting = false
    /// A payment came while the sends ran: they read the waiting payments again before they end.
    private var isKicked = false
    private var workerStarts: [String: Task<Void, Never>] = [:]
    private var listeners: [UUID: AsyncStream<Event>.Continuation] = [:]
    private var monitors: [UUID: AsyncStream<[ProductChatPayment]>.Continuation] = [:]

    init(
        store: any ProductChatPaymentStoring,
        delivery: any ProductPaymentDelivering,
        worker: any ProductHopUploading,
        settings: Settings = Settings(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.store = store
        self.delivery = delivery
        self.worker = worker
        self.settings = settings
        self.logger = logger
    }

    deinit {
        runner?.cancel()
        workerStarts.values.forEach { $0.cancel() }
    }

    /// The events of every payment, from now.
    func events() -> AsyncStream<Event> {
        let id = UUID()
        let (stream, continuation) = AsyncStream<Event>.makeStream(bufferingPolicy: .unbounded)
        listeners[id] = continuation
        continuation.onTermination = { [weak self] _ in
            Task { await self?.removeListener(id) }
        }
        return stream
    }

    /// The payments of a room, oldest first.
    func payments(productId: String, roomId: String) async -> [ProductChatPayment] {
        do {
            return try await store.payments().filter { $0.productId == productId && $0.roomId == roomId }
        } catch {
            logger.error("[payments] cannot read the payments of \(roomId): \(error)")
            return []
        }
    }

    /// Keeps a payment of the user with the handoff of its coins, which `onSaved` commits in the same transaction,
    /// and sends it to the product.
    func send(
        _ memo: TransferMemo,
        messageId: Chat.MessageId,
        timestamp: UInt64,
        productId: String,
        roomId: String,
        onSaved: @escaping (any DurableTxRegistrationScope) throws -> Void
    ) async throws {
        let payment = ProductChatPayment(
            messageId: messageId,
            productId: productId,
            roomId: roomId,
            incoming: false,
            timestamp: timestamp,
            totalValue: memo.totalValue,
            coinKeys: memo.entries,
            delivery: .sending,
            state: nil
        )

        try await store.saveOutgoing(payment, onSaved: onSaved)
        emit(.staged(payment))
        await notifyMonitors()
        kick()
    }

    /// Takes the coins of a payment that a contact sent (`payment_received`). The product can ask again for the
    /// same message: the host keeps it once. A payment that comes before the coinage services run waits here for the
    /// transfer monitor, as a native payment message waits in the chat.
    func receive(productId: String, payment request: HostPersonChatPaymentReceivedRequest) async throws
        -> HostChatPaymentReceipt {
        let payment = ProductChatPayment(
            messageId: request.messageId,
            productId: productId,
            roomId: request.roomId,
            incoming: true,
            timestamp: request.timestamp,
            totalValue: BigUInt(request.totalValue) ?? 0,
            coinKeys: request.coinKeys,
            delivery: .delivered,
            state: nil
        )

        if try await store.saveIncoming(payment) {
            logger.debug("[payments] \(request.roomId) \(request.messageId) received")
            await notifyMonitors()
        }
        return .accepted
    }

    /// Takes over the payments of the native chat (HX3): the monitor claims the coins of those that wait for a claim
    /// and follows those still on their way, under the same message ids, and the rooms draw their states. Gives how
    /// many it took; the payments it kept before are left as they are.
    @discardableResult
    func adopt(_ payments: [AdoptedChatPayment]) async throws -> Int {
        let adopted = try await store.adopt(payments)
        guard !adopted.isEmpty else { return 0 }

        for payment in adopted {
            if let state = payment.state {
                emit(.state(messageId: payment.messageId, state))
            }
        }
        await notifyMonitors()
        return adopted.count
    }

    /// Sends the payments that wait from before, as the native chat sends its waiting messages when it starts.
    func resume() {
        kick()
    }

    /// Waits until no payment goes. For tests.
    func waitUntilIdle() async {
        await runner?.value
    }
}

// MARK: - Transfer states

extension ProductChatPayments: ProductChatPaymentsMonitoring, TransferStateStoring {
    func unsettledPayments() async -> AsyncStream<[ProductChatPayment]> {
        let id = UUID()
        let (stream, continuation) = AsyncStream<[ProductChatPayment]>.makeStream(
            bufferingPolicy: .bufferingNewest(1)
        )
        monitors[id] = continuation
        continuation.onTermination = { [weak self] _ in
            Task { await self?.removeMonitor(id) }
        }
        await continuation.yield(unsettled())
        return stream
    }

    nonisolated var transferStates: any TransferStateStoring {
        self
    }

    func beginIncoming(messageId: Chat.MessageId) async throws -> Date {
        let firstAttemptAt = try await store.beginIncoming(messageId: messageId)
        await emitState(of: messageId)
        return firstAttemptAt
    }

    func updateIncoming(messageId: Chat.MessageId, state: IncomingTransferState) async throws {
        try await store.updateIncoming(messageId: messageId, state: state)
        emit(.state(messageId: messageId, .incoming(state)))
        await notifyMonitors()
    }

    func updateOutgoing(messageId: Chat.MessageId, state: OutgoingTransferState) async throws {
        try await store.updateOutgoing(messageId: messageId, state: state)
        emit(.state(messageId: messageId, .outgoing(state)))
        await notifyMonitors()
    }
}

// MARK: - Delivery

private extension ProductChatPayments {
    /// Starts the sends, or ends the wait of a send that waits to go again.
    func kick() {
        if runner != nil, !isWaiting {
            isKicked = true
            return
        }

        runner?.cancel()
        let id = UUID()
        runnerId = id
        isWaiting = false
        runner = Task { [weak self] in
            await self?.run(id)
            await self?.finish(id)
        }
    }

    func finish(_ id: UUID) {
        guard runnerId == id else { return }
        runner = nil
        runnerId = nil
        isWaiting = false

        if isKicked {
            isKicked = false
            kick()
        }
    }

    /// Sends the waiting payments one after the other. A send that fails waits, and then all go again.
    func run(_ id: UUID) async {
        var delay = settings.firstRetryDelay

        while !Task.isCancelled {
            isKicked = false
            let waiting: [ProductChatPayment]
            do {
                waiting = try await store.payments().filter { !$0.incoming && $0.delivery == .sending }
            } catch {
                logger.error("[payments] cannot read the waiting payments: \(error)")
                return
            }

            guard !waiting.isEmpty else {
                return
            }

            var failed = false
            for payment in waiting where !Task.isCancelled {
                followWorkerStarts(of: payment.productId)
                if await process(payment) == .retry {
                    failed = true
                }
            }

            guard failed else {
                delay = settings.firstRetryDelay
                continue
            }

            guard runnerId == id else { return }
            isWaiting = true
            do {
                try await Task.sleep(for: delay)
            } catch {
                return
            }
            isWaiting = false
            delay = min(delay * 2, settings.maxRetryDelay)
        }
    }

    func process(_ payment: ProductChatPayment) async -> Outcome {
        await worker.holdWorker(productId: payment.productId)
        defer { worker.releaseWorker(productId: payment.productId) }

        do {
            let created = try await delivery.deliver(payment)
            try await store.setDelivery(.delivered, messageId: payment.messageId)

            var delivered = payment
            delivered.delivery = .delivered
            emit(.delivered(delivered, created: created))
            return .delivered
        } catch let refusal as ProductOutboxRefusal {
            logger.warning("[payments] \(payment.roomId) \(payment.messageId) refused: \(refusal.reason ?? "")")
            do {
                try await store.setDelivery(.refused, messageId: payment.messageId)
            } catch {
                logger.error("[payments] \(payment.messageId) not marked refused: \(error)")
                return .retry
            }

            var refused = payment
            refused.delivery = .refused
            emit(.refused(refused))
            await notifyMonitors()
            return .refused
        } catch {
            logger.warning("[payments] \(payment.roomId) \(payment.messageId) goes again: \(error)")
            return .retry
        }
    }

    /// Sends the waiting payments again each time the worker of the product starts.
    func followWorkerStarts(of productId: String) {
        guard workerStarts[productId] == nil else { return }

        let starts = worker.workerStarts(productId: productId)
        workerStarts[productId] = Task { [weak self] in
            for await _ in starts {
                await self?.kick()
            }
        }
    }
}

// MARK: - Listeners

private extension ProductChatPayments {
    func unsettled() async -> [ProductChatPayment] {
        do {
            return try await store.payments().filter(\.isUnsettled)
        } catch {
            logger.error("[payments] cannot read the unsettled payments: \(error)")
            return []
        }
    }

    func notifyMonitors() async {
        guard !monitors.isEmpty else { return }

        let payments = await unsettled()
        for monitor in monitors.values {
            monitor.yield(payments)
        }
    }

    func emitState(of messageId: Chat.MessageId) async {
        guard let state = try? await store.payments().first(where: { $0.messageId == messageId })?.state else {
            return
        }
        emit(.state(messageId: messageId, state))
    }

    func emit(_ event: Event) {
        for listener in listeners.values {
            listener.yield(event)
        }
    }

    func removeListener(_ id: UUID) {
        listeners[id] = nil
    }

    func removeMonitor(_ id: UUID) {
        monitors[id] = nil
    }
}
