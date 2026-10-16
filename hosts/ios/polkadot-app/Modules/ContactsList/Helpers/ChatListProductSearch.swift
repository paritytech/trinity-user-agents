import Foundation
import TrUAPIHost

/// Asks the product-source rooms that declare `search` for the search of the chat list (VERBOSITY-WASM-PRODUCT C11).
protocol ChatListProductSearching: Sendable {
    /// The hits of the rooms, newest first. A room whose product does not run, fails or answers late gives none.
    func search(_ text: String, in rooms: [Chat.Id]) async -> [ChatSearchHit]
}

/// Asks at most ``roomLimit`` rooms at the same time, ``pageSize`` hits for each room. The search ends at
/// ``deadline`` with the hits that arrived, so a slow worker does not stop the list. Only a product whose worker runs
/// now gets the query, so a search never starts a worker.
struct ChatListProductSearch: ChatListProductSearching {
    static let roomLimit = 8
    static let pageSize: UInt32 = 10
    static let deadline = Duration.seconds(4)

    let searching: @Sendable (ChatExtension.Id) -> ProductChatListSearching?
    let roomLimit: Int
    let deadline: Duration
    let logger: LoggerProtocol

    init(registry: ChatExtensionsRegistering, logger: LoggerProtocol) {
        self.init(searching: { RegistryProductRoom(extensionId: $0, registry: registry) }, logger: logger)
    }

    init(
        searching: @escaping @Sendable (ChatExtension.Id) -> ProductChatListSearching?,
        roomLimit: Int = Self.roomLimit,
        deadline: Duration = Self.deadline,
        logger: LoggerProtocol
    ) {
        self.searching = searching
        self.roomLimit = roomLimit
        self.deadline = deadline
        self.logger = logger
    }

    func search(_ text: String, in rooms: [Chat.Id]) async -> [ChatSearchHit] {
        let queries = rooms.prefix(roomLimit).compactMap { chatId -> RoomQuery? in
            guard case let .chatExtension(productId, roomId?) = chatId, let searching = searching(productId) else {
                return nil
            }
            return RoomQuery(chatId: chatId, roomId: roomId, searching: searching)
        }
        guard !queries.isEmpty else { return [] }

        let collector = HitCollector(expected: queries.count)
        let tasks = queries.map { query in
            Task { [logger] in
                let (chatId, roomId, searching) = (query.chatId, query.roomId, query.searching)
                let request = ProductChatSearchQueryRequest(
                    roomId: roomId,
                    text: text,
                    cursor: nil,
                    pageSize: Self.pageSize
                )
                var hits: [ChatSearchHit] = []
                do {
                    if let answer = try await searching.searchPageIfRunning(request) {
                        hits = ProductChatRoomSearch.hits(of: answer, chatId: chatId, limit: Int(Self.pageSize))
                    }
                } catch {
                    logger.warning("The list search of room \(roomId) got no answer: \(error)")
                }
                await collector.add(hits)
            }
        }
        let timer = Task { [deadline] in
            try? await Task.sleep(for: deadline)
            await collector.finish()
        }

        let hits = await withTaskCancellationHandler {
            await collector.hits()
        } onCancel: {
            Task { await collector.finish() }
        }

        tasks.forEach { $0.cancel() }
        timer.cancel()
        return hits.sorted { $0.timestamp > $1.timestamp }
    }
}

/// One room that the search of the list asks.
private struct RoomQuery {
    let chatId: Chat.Id
    let roomId: String
    let searching: ProductChatListSearching
}

/// Keeps the hits of the rooms until every room answered or the search ends.
private actor HitCollector {
    private let expected: Int
    private var answered = 0
    private var collected: [ChatSearchHit] = []
    private var finished = false
    private var waiter: CheckedContinuation<[ChatSearchHit], Never>?

    init(expected: Int) {
        self.expected = expected
    }

    func add(_ hits: [ChatSearchHit]) {
        guard !finished else { return }
        collected += hits
        answered += 1
        if answered >= expected {
            finish()
        }
    }

    func finish() {
        guard !finished else { return }
        finished = true
        waiter?.resume(returning: collected)
        waiter = nil
    }

    func hits() async -> [ChatSearchHit] {
        if finished {
            return collected
        }
        return await withCheckedContinuation { waiter = $0 }
    }
}

extension ChatListSearchResults {
    /// The results with the hits of the product-source rooms: the newest hits first, at most `limit`.
    func adding(productHits: [ChatSearchHit], chats: [Chat.Id: ChatWithPeerMetadata], limit: Int) -> Self {
        let added = productHits.compactMap { hit in chats[hit.chatId].map { ChatListMessageHit(hit: hit, chat: $0) } }
        let merged = (hits + added).sorted { $0.hit.timestamp > $1.hit.timestamp }
        return ChatListSearchResults(query: query, chats: self.chats, hits: Array(merged.prefix(limit)))
    }
}
