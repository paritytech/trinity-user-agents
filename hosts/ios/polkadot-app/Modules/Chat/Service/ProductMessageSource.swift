import Foundation
import AsyncExtensions
import StructuredConcurrency
import TrUAPIHost

/// The messages of a product-source room: a window of pages that the product gives.
///
/// The window opens on the newest page. `loadOlder` adds the page before the window and `loadNewer` the page
/// after it. The window holds at most `Settings.windowLimit` messages, so a new page at one end drops whole
/// pages at the other end. The source keeps the request of each dropped page and asks it again when the user
/// comes back. Feed updates change the window in place. The actions of the user go to the product as
/// `user_action` queries, and the window shows their result before the answer arrives.
actor ProductMessageSource: ChatMessageSourcing, ProductFeedListening {
    struct Settings: Sendable {
        var pageSize: UInt32 = 50
        var windowLimit = 500
        /// The wait before the one retry of a page that the product could not give.
        var retryDelay: Duration = .seconds(1)
        /// The most pages that one jump asks in each direction.
        var jumpPageLimit = 1_000
        /// The product wants a `Seen` action when the user sees a newer message.
        var reportsSeen = false
        /// The wait before a `Seen` that the product did not take goes again.
        var seenRetryDelay: Duration = .seconds(3)
        /// The most times that one `Seen` goes to the product.
        var seenAttempts = 3
    }

    struct Page {
        /// The request that gave the page. The source asks it again after it drops the page.
        let anchor: ChatPageAnchor
        /// The cursor of the page before this one. Nil when the product has no older message.
        let next: String?
        var ids: [String]
    }

    /// How a page request ended after its retry.
    enum PageResult {
        case page(ProductChatMessagesQueryItem)
        /// The product cannot give pages for this end (`NotAvailable`).
        case notAvailable
        /// The product did not answer in time, or refused the room.
        case failed
        /// The worker of the product does not answer.
        case unavailable
    }

    nonisolated let chatId: Chat.Id
    nonisolated let productId: ChatExtension.Id
    nonisolated let roomId: String

    let querying: ProductChatQuerying
    let cache: ProductMessageCaching
    let settings: Settings
    let now: @Sendable () -> UInt64
    /// The outbox of the messages of the user. Its messages of this room show at the end of the window until
    /// the product has them.
    let outbox: ProductOutbox
    let logger: LoggerProtocol

    let subject: AsyncCurrentValueSubject<MessageListSnapshot>
    var firstPageTask: Task<Void, Never>?
    var olderTask: Task<ChatPageOutcome, Never>?
    var newerTask: Task<ChatPageOutcome, Never>?
    var outboxTask: Task<Void, Never>?

    var pages: [Page] = []
    var messages: [String: ChatMessage] = [:]
    /// The messages of the user in the outbox, which the product does not have yet, oldest first.
    var pending: [String] = []
    /// The pages that the window dropped at each end. The last page is next to the window.
    var droppedOlder: [ChatPageAnchor] = []
    var droppedNewer: [ChatPageAnchor] = []
    var olderFailed = false
    var newerFailed = false
    /// The product cannot give older pages.
    var olderClosed = false
    var readiness = ChatRoomReadiness.loading
    var hasFirstPage = false
    var started = false

    var seenMessageId: String?
    var seenTimestamp: UInt64?
    var reportedSeenMessageId: String?
    var seenToReport: String?
    var isReportingSeen = false
    /// How many times the source sent the `Seen` of ``seenToReport`` with no answer of the product.
    var failedSeenAttempts = 0

    init(
        chatId: Chat.Id,
        productId: ChatExtension.Id,
        roomId: String,
        querying: ProductChatQuerying,
        cache: ProductMessageCaching = ProductMessageCache.shared,
        settings: Settings = Settings(),
        now: @escaping @Sendable () -> UInt64 = { Date().toChatTimestamp() },
        outbox: ProductOutbox? = nil,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.chatId = chatId
        self.productId = productId
        self.roomId = roomId
        self.querying = querying
        self.cache = cache
        self.settings = settings
        self.now = now
        // A room without the outbox of the app sends through an outbox of its own.
        self.outbox = outbox ?? .inMemory(querying: querying, logger: logger)
        self.logger = logger
        subject = AsyncCurrentValueSubject(MessageListSnapshot(
            messages: [],
            paging: ChatPagingState(older: .loading, newer: .end),
            readiness: .loading
        ))
    }

    deinit {
        firstPageTask?.cancel()
        olderTask?.cancel()
        newerTask?.cancel()
        outboxTask?.cancel()
    }

    // MARK: - ProductFeedListening

    func apply(roomId: String, update: ChatFeedUpdate) async {
        guard roomId == self.roomId else {
            return
        }

        apply(update)
    }

    // MARK: - ChatMessageSourcing

    func snapshots() async throws -> AnyAsyncSequence<MessageListSnapshot> {
        await start()
        return subject.eraseToAnyAsyncSequence()
    }

    /// The current window.
    var snapshot: MessageListSnapshot {
        subject.value
    }

    /// Shows the cached page and asks for the newest page. A second call does nothing.
    func start() async {
        guard !started else {
            return
        }

        started = true

        if let entry = await cache.entry(productId: productId, roomId: roomId) {
            restore(from: entry)
        }

        await followOutbox()
        emit()

        firstPageTask = Task { [weak self] in
            guard let self else {
                return
            }
            await loadNewestPage()
        }
    }

    #if DEBUG
        /// Waits until the answer for the first page arrived. Only the tests use it.
        func waitForFirstPage() async {
            await firstPageTask?.value
        }
    #endif

    func loadOlder() async -> ChatPageOutcome {
        if let olderTask {
            return await olderTask.value
        }

        guard let anchor = olderAnchor else {
            return .end
        }

        let task = Task { await self.pausingAfterFailure(self.fetchOlder(anchor)) }
        olderTask = task
        olderFailed = false
        emit()

        let outcome = await task.value
        olderTask = nil
        emit()
        return outcome
    }

    func loadNewer() async -> ChatPageOutcome {
        if let newerTask {
            return await newerTask.value
        }

        guard let anchor = droppedNewer.last else {
            return .end
        }

        let task = Task { await self.pausingAfterFailure(self.fetchNewer(anchor)) }
        newerTask = task
        newerFailed = false
        emit()

        let outcome = await task.value
        newerTask = nil
        emit()
        return outcome
    }

    /// A page that failed keeps its end loading for `retryDelay`. The room screen shows no row for it and asks it
    /// again when the user scrolls near the end, so it is asked again at most once in that time.
    func pausingAfterFailure(_ outcome: ChatPageOutcome) async -> ChatPageOutcome {
        switch outcome {
        case .failed,
             .unavailable:
            try? await Task.sleep(for: settings.retryDelay)
        case .appended,
             .end:
            break
        }
        return outcome
    }

    /// Pages until the window holds the message. A jump before the room screen starts the source starts it.
    func jump(to messageId: Chat.MessageId) async -> Bool {
        await start()
        await firstPageTask?.value

        if await page(until: messageId, using: loadOlder) {
            return true
        }

        return await page(until: messageId, using: loadNewer)
    }

    /// Shows "unavailable" and keeps the window, for example when the worker of the product cannot start.
    func markUnavailable() {
        readiness = .unavailable
        emit()
    }

    func reload() async {
        firstPageTask?.cancel()
        let task = Task { [weak self] in
            guard let self else {
                return
            }
            await loadNewestPage()
        }
        firstPageTask = task
        await task.value
    }

    func perform(_ action: ChatSourceAction) async throws -> ChatSourceActionOutcome {
        switch action {
        case let .send(.edited(edit)):
            return await perform(.editMessage(
                messageId: edit.messageId,
                body: editedBody(of: edit.messageId, text: edit.newContent.text ?? "")
            ))
        case let .send(content):
            guard let userAction = content.productUserAction else {
                throw ChatMessageSourceError.unsupported
            }
            return await perform(userAction)
        case let .markSeen(messageIds):
            markSeen(messageIds)
            return .done
        case let .toggleReaction(messageId, emoji):
            let hasReaction = messages[messageId]?.reactions.contains { $0.emoji == emoji && $0.reactedByUser } ?? false
            return await perform(hasReaction
                ? .reactionRemoved(messageId: messageId, emoji: emoji)
                : .reactionPlaced(messageId: messageId, emoji: emoji))
        }
    }

    // MARK: - Feed updates

    /// Changes the window as the product asks.
    func apply(_ update: ChatFeedUpdate) {
        switch update {
        case let .new(message):
            insertLive(message)
        case let .edited(messageId, body, editedAt):
            modify(messageId) {
                $0.body = body
                $0.editedAt = editedAt
            }
        case let .replaced(message):
            modify(message.messageId) { $0 = message }
        case let .reactionPlaced(messageId, participantId, emoji):
            modify(messageId) { $0.adjustReaction(emoji, by: participantId, placed: true) }
        case let .reactionRemoved(messageId, participantId, emoji):
            modify(messageId) { $0.adjustReaction(emoji, by: participantId, placed: false) }
        case let .statusChanged(messageId, status):
            modify(messageId) { $0.status = status }
        }

        emit()
    }
}
