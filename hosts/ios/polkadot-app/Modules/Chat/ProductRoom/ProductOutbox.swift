import Foundation
import StructuredConcurrency
import TrUAPIHost

/// A message of the user in a product room that the product does not have yet. Its id is the message id that the
/// host chose, a UUID. The product sends the message under that id, so the message keeps its id once the product
/// has it, and a second send with the same id gives the same message.
struct ProductOutgoingMessage: Codable, Equatable, Sendable {
    enum Content: Codable, Equatable, Sendable {
        /// A text and the message it replies to.
        case text(String, replyTo: String?)
    }

    enum State: Codable, Equatable, Sendable {
        /// The host sends the message, or waits to send it again.
        case sending
        /// The product refused the message. It stays unsent.
        case refused
    }

    /// The id of the message, in the window of the room and in the product.
    let id: String
    let productId: String
    let roomId: String
    let content: Content
    /// Milliseconds since the Unix epoch.
    let createdAt: UInt64
    var state: State

    var replyTo: String? {
        if case let .text(_, replyTo) = content { replyTo } else { nil }
    }
}

/// What happened to a message of the outbox, for the window of the room.
enum ProductOutboxEvent: Equatable {
    case changed(ProductOutgoingMessage)
    /// The product has the message. `created` is the message that the product made.
    case delivered(ProductOutgoingMessage, created: ChatMessage?)
}

/// Keeps the worker of the product while the outbox sends, and tells when the worker starts.
protocol ProductOutboxWorkers: Sendable {
    /// Keeps the worker of the product while the host sends, also after the room screen closes.
    func holdWorker(productId: String) async
    func releaseWorker(productId: String)
    /// Each start of the worker of the product, from now. The outbox sends its waiting messages again.
    func workerStarts(productId: String) -> AsyncStream<Void>
}

/// Gives a message of the outbox to the product.
protocol ProductOutboxDelivering: Sendable {
    /// The message that the product made, or nil when it only acknowledged the message. Throws
    /// ``ProductOutboxRefusal`` when the product refused it.
    func deliver(_ message: ProductOutgoingMessage, body: ChatMessageBody) async throws -> ChatMessage?
}

/// The product refused a message, or the message cannot go: it does not go again.
struct ProductOutboxRefusal: Error, Equatable {
    let reason: String?
}

/// Keeps each message of the user until the product has it. The work does not
/// depend on the room screen: it continues after the screen closes. The messages of a room go in the order of the
/// user. A send that fails goes again after a wait, when the worker of the product starts again, when the room opens
/// and when the app starts.
actor ProductOutbox {
    struct Settings: Sendable {
        /// The wait before the first new send of a message that did not go. Each next wait doubles.
        var firstRetryDelay: Duration = .seconds(2)
        var maxRetryDelay: Duration = .seconds(60)
    }

    static let shared = ProductOutbox(
        directory: ProductOutboxDirectory.shared,
        workers: TrUAPIProductOutboxWorkers(),
        delivery: ProductOutboxDelivery.shared
    )

    fileprivate struct RoomKey: Hashable {
        let productId: String
        let roomId: String

        init(_ message: ProductOutgoingMessage) {
            productId = message.productId
            roomId = message.roomId
        }

        init(productId: String, roomId: String) {
            self.productId = productId
            self.roomId = roomId
        }
    }

    fileprivate struct Runner {
        let id: UUID
        let task: Task<Void, Never>
        /// The runner waits before it sends again.
        var isWaiting = false
    }

    fileprivate enum Outcome {
        case delivered
        case refused
        case retry
    }

    private let directory: ProductOutboxStoring
    private let workers: ProductOutboxWorkers
    private let delivery: ProductOutboxDelivering
    private let settings: Settings
    private let now: @Sendable () -> UInt64
    private let logger: LoggerProtocol

    private var runners: [RoomKey: Runner] = [:]
    private var workerStarts: [String: Task<Void, Never>] = [:]
    private var listeners: [UUID: AsyncStream<ProductOutboxEvent>.Continuation] = [:]

    init(
        directory: ProductOutboxStoring,
        workers: ProductOutboxWorkers,
        delivery: ProductOutboxDelivering,
        settings: Settings = Settings(),
        now: @escaping @Sendable () -> UInt64 = { Date().toChatTimestamp() },
        logger: LoggerProtocol = Logger.shared
    ) {
        self.directory = directory
        self.workers = workers
        self.delivery = delivery
        self.settings = settings
        self.now = now
        self.logger = logger
    }

    deinit {
        runners.values.forEach { $0.task.cancel() }
        workerStarts.values.forEach { $0.cancel() }
    }

    /// An outbox in memory that gives the messages to the product through `querying`. A room whose product gives no
    /// outbox sends with it.
    static func inMemory(
        querying: ProductChatQuerying,
        settings: Settings = Settings(),
        logger: LoggerProtocol = Logger.shared
    ) -> ProductOutbox {
        ProductOutbox(
            directory: ProductOutboxMemory(),
            workers: NoProductOutboxWorkers(),
            delivery: ProductQueryingDelivery(querying: querying),
            settings: settings,
            logger: logger
        )
    }

    /// The events of every message, from now.
    func events() -> AsyncStream<ProductOutboxEvent> {
        let id = UUID()
        let (stream, continuation) = AsyncStream<ProductOutboxEvent>.makeStream(bufferingPolicy: .unbounded)
        listeners[id] = continuation
        continuation.onTermination = { [weak self] _ in
            Task { await self?.removeListener(id) }
        }
        return stream
    }

    /// The messages of a room, oldest first. The room sends its waiting messages now.
    func entries(productId: String, roomId: String) -> [ProductOutgoingMessage] {
        let key = RoomKey(productId: productId, roomId: roomId)
        kick(key)
        return directory.entries().filter { RoomKey($0) == key }
    }

    /// Sends the waiting messages of every room.
    func resume() {
        Set(directory.entries().filter { $0.state == .sending }.map(RoomKey.init)).forEach(kick)
    }

    /// Keeps a text and sends it. Gives the message, whose id the host chose.
    @discardableResult
    func send(
        text: String,
        replyTo: String?,
        productId: String,
        roomId: String
    ) throws -> ProductOutgoingMessage {
        try add(.text(text, replyTo: replyTo), productId: productId, roomId: roomId)
    }

    /// Waits until the product has the message. Gives false when the product refused it.
    func waitUntilDelivered(_ id: String) async -> Bool {
        let events = events()

        guard let message = directory.entry(id) else {
            return true
        }

        if message.state == .refused {
            return false
        }

        for await event in events {
            switch event {
            case let .delivered(delivered, _) where delivered.id == id:
                return true
            case let .changed(changed) where changed.id == id && changed.state == .refused:
                return false
            default:
                continue
            }
        }

        return false
    }

    #if DEBUG
        /// Waits until the room sends nothing. Only the tests use it.
        func waitUntilIdle(productId: String, roomId: String) async {
            await runners[RoomKey(productId: productId, roomId: roomId)]?.task.value
        }
    #endif
}

// MARK: - Pipeline

private extension ProductOutbox {
    func add(
        _ content: ProductOutgoingMessage.Content,
        productId: String,
        roomId: String
    ) throws -> ProductOutgoingMessage {
        let message = ProductOutgoingMessage(
            id: UUID().uuidString,
            productId: productId,
            roomId: roomId,
            content: content,
            createdAt: now(),
            state: .sending
        )
        try directory.save(message)
        kick(RoomKey(message))
        return message
    }

    /// Starts the sends of a room, or ends the wait of a room that waits to send again.
    func kick(_ key: RoomKey) {
        followWorkerStarts(of: key.productId)

        guard let runner = runners[key] else {
            start(key)
            return
        }

        if runner.isWaiting {
            runner.task.cancel()
            start(key)
        }
    }

    func start(_ key: RoomKey) {
        let id = UUID()
        let task = Task { [weak self] in
            await self?.run(key, runner: id)
            await self?.finish(key, runner: id)
        }
        runners[key] = Runner(id: id, task: task)
    }

    func finish(_ key: RoomKey, runner id: UUID) {
        if runners[key]?.id == id {
            runners[key] = nil
        }
    }

    func setWaiting(_ isWaiting: Bool, _ key: RoomKey, runner id: UUID) {
        if runners[key]?.id == id {
            runners[key]?.isWaiting = isWaiting
        }
    }

    /// Sends the messages of the room one after the other. A send that fails waits, and then goes again.
    func run(_ key: RoomKey, runner id: UUID) async {
        var delay = settings.firstRetryDelay

        while !Task.isCancelled {
            guard let message = directory.entries().first(where: { RoomKey($0) == key && $0.state == .sending }) else {
                return
            }

            switch await process(message) {
            case .delivered,
                 .refused:
                delay = settings.firstRetryDelay
            case .retry:
                setWaiting(true, key, runner: id)
                do {
                    try await Task.sleep(for: delay)
                } catch {
                    return
                }
                setWaiting(false, key, runner: id)
                delay = min(delay * 2, settings.maxRetryDelay)
            }
        }
    }

    func process(_ message: ProductOutgoingMessage) async -> Outcome {
        var message = message
        emit(.changed(message))

        await workers.holdWorker(productId: message.productId)
        defer { workers.releaseWorker(productId: message.productId) }

        do {
            let created = try await delivery.deliver(message, body: message.body)

            directory.remove(message.id)
            emit(.delivered(message, created: created))
            return .delivered
        } catch let refusal as ProductOutboxRefusal {
            logger.warning("[outbox] \(message.roomId) \(message.id) refused: \(refusal.reason ?? "")")
            message.state = .refused
            try? directory.update(message)
            emit(.changed(message))
            return .refused
        } catch {
            logger.warning("[outbox] \(message.roomId) \(message.id) goes again: \(error)")
            return .retry
        }
    }

    /// Sends the waiting messages of the product again each time its worker starts.
    func followWorkerStarts(of productId: String) {
        guard workerStarts[productId] == nil else { return }

        let starts = workers.workerStarts(productId: productId)
        workerStarts[productId] = Task { [weak self] in
            for await _ in starts {
                await self?.kickRooms(of: productId)
            }
        }
    }

    func kickRooms(of productId: String) {
        Set(directory.entries().filter { $0.productId == productId && $0.state == .sending }.map(RoomKey.init))
            .forEach(kick)
    }

    func emit(_ event: ProductOutboxEvent) {
        for listener in listeners.values {
            listener.yield(event)
        }
    }

    func removeListener(_ id: UUID) {
        listeners[id] = nil
    }
}

private extension ProductOutgoingMessage {
    /// The body that the product gets.
    var body: ChatMessageBody {
        switch content {
        case let .text(text, _):
            .text(text: text)
        }
    }
}
