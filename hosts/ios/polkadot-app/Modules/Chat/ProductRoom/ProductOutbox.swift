import Foundation
import StructuredConcurrency
import SubstrateSdk
import TrUAPIHost

/// A message of the user in a room of the chat product that the product does not have yet: a text, or a photo or a
/// video with its caption (P10.3). Its id is the message id that the host chose, a UUID as the native chat names its
/// messages. The product sends the message under that id, so the message keeps its id once the product has it, and
/// a second send with the same id gives the same message.
struct ProductOutgoingMessage: Codable, Equatable, Sendable {
    enum Content: Codable, Equatable, Sendable {
        /// A text, the message it replies to, and whether it is the welcome message of a chat request.
        case text(String, replyTo: String?, request: Bool)
        /// The file in the store of the uploads of the native chat, the SCALE bytes of its metadata
        /// (`ChatRemoteMessageContent.FileMeta`), and the caption.
        case media(fileId: String, meta: Data, caption: String?)
    }

    enum State: Codable, Equatable, Sendable {
        /// The host sends the message, or waits to send it again.
        case sending
        /// The product refused the message. It stays unsent, as a message of the native chat whose send cannot go
        /// again stays.
        case refused
    }

    /// The id of the message, in the window of the room and in the product.
    let id: String
    let productId: String
    let roomId: String
    let content: Content
    /// Milliseconds since the Unix epoch.
    let createdAt: UInt64
    /// The HOP file, after the core uploaded the bytes. A second send then uploads nothing.
    var hopFile: ChatHopResource?
    var state: State

    var replyTo: String? {
        if case let .text(_, replyTo, _) = content { replyTo } else { nil }
    }

    var fileMeta: ChatRemoteMessageContent.FileMeta? {
        guard case let .media(_, meta, _) = content else { return nil }
        return try? ChatRemoteMessageContent.FileMeta(scaleDecoder: ScaleDecoder(data: meta))
    }

    /// The photo or the video while it is on its way, as the native chat draws an upload. Nil for a text.
    func localMessage(chatId: Chat.Id) -> Chat.LocalMessage? {
        guard case let .media(fileId, _, caption) = content, let fileMeta else { return nil }

        let attachment = Chat.LocalMessage.Content.LocalUploadableFile(
            relativeLocalPath: fileId,
            meta: fileMeta,
            uploadingInfo: nil
        )

        return Chat.LocalMessage(
            messageId: id,
            chatId: chatId,
            origin: .user,
            creationSource: .localDevice,
            status: .outgoing(.new),
            timestamp: createdAt,
            content: .richText(.init(text: caption, attachments: [.localUploadable(attachment)])),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
    }
}

/// What happened to a message of the outbox, for the window of the room.
enum ProductOutboxEvent: Equatable {
    case changed(ProductOutgoingMessage)
    /// The product has the message. `created` is the message that the product made.
    case delivered(ProductOutgoingMessage, created: ChatMessage?)
}

/// Uploads the bytes of a file to a HOP node through the core, which holds the keys of the app's HOP nodes, and keeps
/// the worker of the product while the outbox sends.
protocol ProductHopUploading: Sendable {
    /// `progress` gets the bytes sent and the size of the file while the upload runs.
    func upload(
        productId: String,
        roomId: String,
        bytes: Data,
        progress: @escaping @Sendable (_ sent: UInt64, _ total: UInt64) -> Void
    ) async throws -> ChatHopResource
    /// Keeps the worker of the product while the host uploads and sends, also after the room screen closes.
    func holdWorker(productId: String) async
    func releaseWorker(productId: String)
    /// Each start of the worker of the product, from now. The outbox sends its waiting messages again, as the native
    /// chat sends its waiting messages when it connects again.
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

/// Keeps each message of the user until the product has it, as the outbox of the native chat does. The work does not
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
        uploads: AttachmentStore.uploads(),
        hop: TrUAPIProductHopUploads(),
        delivery: ProductOutboxDelivery.shared
    )

    nonisolated let progress = ProductMediaProgress()

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
    private let uploads: AttachmentStoring?
    private let hop: ProductHopUploading
    private let delivery: ProductOutboxDelivering
    private let downloads: ProductMediaDownloads
    private let settings: Settings
    private let now: @Sendable () -> UInt64
    private let logger: LoggerProtocol

    private var runners: [RoomKey: Runner] = [:]
    private var workerStarts: [String: Task<Void, Never>] = [:]
    private var listeners: [UUID: AsyncStream<ProductOutboxEvent>.Continuation] = [:]

    init(
        directory: ProductOutboxStoring,
        uploads: AttachmentStoring?,
        hop: ProductHopUploading,
        delivery: ProductOutboxDelivering,
        downloads: ProductMediaDownloads = .shared,
        settings: Settings = Settings(),
        now: @escaping @Sendable () -> UInt64 = { Date().toChatTimestamp() },
        logger: LoggerProtocol = Logger.shared
    ) {
        self.directory = directory
        self.uploads = uploads
        self.hop = hop
        self.delivery = delivery
        self.downloads = downloads
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
            uploads: nil,
            hop: NoProductHopUploads(),
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

    /// Sends the waiting messages of every room, as the native chat does when the app starts.
    func resume() {
        Set(directory.entries().filter { $0.state == .sending }.map(RoomKey.init)).forEach(kick)
    }

    /// Keeps a text and sends it. Gives the message, whose id the host chose.
    @discardableResult
    func send(
        text: String,
        replyTo: String?,
        request: Bool = false,
        productId: String,
        roomId: String
    ) throws -> ProductOutgoingMessage {
        try add(.text(text, replyTo: replyTo, request: request), productId: productId, roomId: roomId)
    }

    /// Keeps the file and sends it with its caption. Gives the message, whose id the host chose.
    @discardableResult
    func send(
        _ attachment: ProcessedAttachment,
        caption: String?,
        productId: String,
        roomId: String
    ) throws -> ProductOutgoingMessage {
        let encoder = ScaleEncoder()
        try attachment.meta.encode(scaleEncoder: encoder)
        return try add(
            .media(fileId: attachment.fileId, meta: encoder.encode(), caption: caption),
            productId: productId,
            roomId: roomId
        )
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

    /// Waits until the room sends nothing. For tests.
    func waitUntilIdle(productId: String, roomId: String) async {
        await runners[RoomKey(productId: productId, roomId: roomId)]?.task.value
    }
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
            hopFile: nil,
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

        await hop.holdWorker(productId: message.productId)
        defer { hop.releaseWorker(productId: message.productId) }

        do {
            let (body, bytes) = try await body(of: &message)
            let created = try await delivery.deliver(message, body: body)

            if let bytes, let variant = body.sentFile {
                await downloads.keep(bytes, for: variant)
            }

            directory.remove(message.id)
            emit(.delivered(message, created: created))
            return .delivered
        } catch let refusal as ProductOutboxRefusal {
            logger.warning("[outbox] \(message.roomId) \(message.id) refused: \(refusal.reason ?? "")")
            await reportFailure(of: message, error: refusal)
            message.state = .refused
            try? directory.update(message)
            emit(.changed(message))
            return .refused
        } catch {
            logger.warning("[outbox] \(message.roomId) \(message.id) goes again: \(error)")
            await reportFailure(of: message, error: error)
            return .retry
        }
    }

    /// The body that the product gets, and the bytes of the file of a photo or a video.
    func body(of message: inout ProductOutgoingMessage) async throws -> (ChatMessageBody, Data?) {
        switch message.content {
        case let .text(text, _, _):
            return (.text(text: text), nil)
        case let .media(fileId, _, caption):
            guard let bytes = try uploads?.loadAttachment(by: fileId), let meta = message.fileMeta else {
                throw ProductOutboxRefusal(reason: nil)
            }

            let attachmentId = AttachmentId(messageId: message.id, fileId: fileId)
            let hopFile = try await hopFile(of: &message, bytes: bytes, attachmentId: attachmentId)
            await progress.report(.onComplete, for: attachmentId)
            return (.richText(text: caption, media: [meta.mediaItem(hopFile)]), bytes)
        }
    }

    func hopFile(
        of message: inout ProductOutgoingMessage,
        bytes: Data,
        attachmentId: AttachmentId
    ) async throws -> ChatHopResource {
        if let hopFile = message.hopFile {
            return hopFile
        }

        // The core reports the bytes sent while the file goes. The reports reach the progress ring in their order,
        // and before the end of the upload, as the native chat draws the progress of an upload.
        let (sent, sending) = AsyncStream<AttachmentProgressEvent.Progress>.makeStream(
            bufferingPolicy: .bufferingNewest(1)
        )
        let reporting = Task { [progress] in
            for await loaded in sent {
                await progress.report(.onProgress(loaded), for: attachmentId)
            }
        }

        let hopFile: ChatHopResource
        do {
            hopFile = try await hop.upload(productId: message.productId, roomId: message.roomId, bytes: bytes) {
                sending.yield(.init(loaded: Int(clamping: $0), total: Int(clamping: $1)))
            }
            sending.finish()
            await reporting.value
        } catch {
            sending.finish()
            await reporting.value
            throw error
        }

        message.hopFile = hopFile
        try? directory.update(message)
        return hopFile
    }

    func reportFailure(of message: ProductOutgoingMessage, error: Error) async {
        guard case let .media(fileId, _, _) = message.content else { return }
        await progress.report(.onFailure(error), for: AttachmentId(messageId: message.id, fileId: fileId))
    }

    /// Sends the waiting messages of the product again each time its worker starts.
    func followWorkerStarts(of productId: String) {
        guard workerStarts[productId] == nil else { return }

        let starts = hop.workerStarts(productId: productId)
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
