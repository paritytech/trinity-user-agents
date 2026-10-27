import Foundation
import os
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The window of a product-source room.
struct ProductMessageSourceTests {
    // MARK: - Pages

    /// The room opens on `Before { None }` with the newest message at the bottom.
    @Test func opensOnTheNewestPage() async {
        let history = ProductRoomHistory(count: 120)
        let querying = ScriptedProductChatQuerying(page: history.answer)
        let source = makeSource(querying)

        await open(source)

        let snapshot = await source.snapshot
        #expect(snapshot.messages.map(\.messageId) == history.ids(70 ..< 120))
        #expect(snapshot.readiness == .ready)
        #expect(snapshot.paging == ChatPagingState(older: .more, newer: .end))
        #expect(querying.pageRequests == [request(.before(cursor: nil))])
    }

    /// Scrolling up pages with the returned cursor until `next: None`, and asks no page twice.
    @Test func pagesOlderWithTheReturnedCursorUntilTheEnd() async {
        let history = ProductRoomHistory(count: 120)
        let querying = ScriptedProductChatQuerying(page: history.answer)
        let source = makeSource(querying)
        await open(source)

        #expect(await source.loadOlder() == .appended(50))
        #expect(await source.loadOlder() == .appended(20))
        #expect(await source.loadOlder() == .end)

        let snapshot = await source.snapshot
        #expect(snapshot.messages.map(\.messageId) == history.ids(0 ..< 120))
        #expect(snapshot.paging.older == .end)
        #expect(querying.pageRequests.map(\.anchor) == [
            .before(cursor: nil),
            .before(cursor: "70"),
            .before(cursor: "20")
        ])
    }

    /// The window never holds more than 500 messages, and a dropped page comes back with its request.
    @Test func theWindowStaysWithinItsBound() async {
        let history = ProductRoomHistory(count: 2_000)
        let querying = ScriptedProductChatQuerying(page: history.answer)
        let source = makeSource(querying)
        await open(source)

        for _ in 0 ..< 20 {
            _ = await source.loadOlder()
            #expect(await source.snapshot.messages.count <= 500)
        }

        let scrolledUp = await source.snapshot
        #expect(scrolledUp.paging.newer == .more)
        #expect(scrolledUp.messages.map(\.messageId) == history.ids(950 ..< 1_450))

        let requestsBefore = querying.pageRequests.count
        while case .appended = await source.loadNewer() {
            #expect(await source.snapshot.messages.count <= 500)
        }

        let scrolledDown = await source.snapshot
        #expect(scrolledDown.messages.map(\.messageId) == history.ids(1_500 ..< 2_000))
        #expect(scrolledDown.paging == ChatPagingState(older: .more, newer: .end))
        #expect(querying.pageRequests.dropFirst(requestsBefore).map(\.anchor) == [
            .before(cursor: "1500"),
            .before(cursor: "1550"),
            .before(cursor: "1600"),
            .before(cursor: "1650"),
            .before(cursor: "1700"),
            .before(cursor: "1750"),
            .before(cursor: "1800"),
            .before(cursor: "1850"),
            .before(cursor: "1900"),
            .before(cursor: "1950"),
            .before(cursor: nil)
        ])
    }

    /// Messages that arrived while the newest page was out of the window come back without a gap.
    @Test func theNewestPageComesBackWithoutAGap() async {
        let history = ProductRoomHistory(count: 600)
        let querying = ScriptedProductChatQuerying(page: history.answer)
        let source = makeSource(querying)
        await open(source)

        for _ in 0 ..< 10 {
            _ = await source.loadOlder()
        }
        #expect(await source.snapshot.paging.newer == .more)

        history.append(30)
        while case .appended = await source.loadNewer() {}

        let ids = await source.snapshot.messages.map(\.messageId)
        #expect(ids.last == "m629")
        #expect(ids == history.ids(630 - ids.count ..< 630))
    }

    /// A jump pages until the message is in the window, and fails only when both ends come first.
    @Test func jumpPagesUntilTheMessageIsInTheWindow() async {
        let history = ProductRoomHistory(count: 2_000)
        let querying = ScriptedProductChatQuerying(page: history.answer)
        let source = makeSource(querying)
        await open(source)

        #expect(await source.jump(to: "m10"))
        let snapshot = await source.snapshot
        #expect(snapshot.messages.contains { $0.messageId == "m10" })
        #expect(snapshot.messages.count <= 500)

        #expect(await !source.jump(to: "missing"))
        #expect(await source.snapshot.paging == ChatPagingState(older: .more, newer: .end))
    }

    @Test func aPageThatTimesOutFailsWithoutARetry() async {
        let querying = ScriptedProductChatQuerying { request in
            guard case .before(cursor: nil) = request.anchor else {
                throw ChatQueryError.timedOut
            }
            return ProductChatMessagesQueryItem(messages: [productMessage(1)], next: "1")
        }
        let source = makeSource(querying)
        await open(source)

        #expect(await source.loadOlder() == .failed)
        #expect(querying.pageRequests.count == 2)
        #expect(await source.snapshot.paging.older == .failed)
        #expect(await source.snapshot.readiness == .ready)
    }

    /// `NotAvailable` gets one retry, then the end has no more pages.
    @Test func notAvailableEndsTheOlderPagesAfterOneRetry() async {
        let querying = ScriptedProductChatQuerying { request in
            guard case .before(cursor: nil) = request.anchor else {
                throw ChatQueryError.refused(.notAvailable)
            }
            return ProductChatMessagesQueryItem(messages: [productMessage(1)], next: "1")
        }
        let source = makeSource(querying)
        await open(source)

        #expect(await source.loadOlder() == .end)
        #expect(querying.pageRequests.count == 3)
        #expect(await source.snapshot.paging.older == .end)
    }

    /// The host retries an `unavailable` interrupt once after a delay.
    @Test func anUnavailableProductGetsOneRetry() async {
        let attempts = OSAllocatedUnfairLock(initialState: 0)
        let querying = ScriptedProductChatQuerying { _ in
            let attempt = attempts.withLock { count in
                count += 1
                return count
            }
            guard attempt > 1 else {
                throw ChatQueryError.failed(reason: "the worker stopped")
            }
            return ProductChatMessagesQueryItem(messages: [productMessage(1)], next: nil)
        }
        let source = makeSource(querying)
        await open(source)

        #expect(querying.pageRequests.count == 2)
        #expect(await source.snapshot.readiness == .ready)
        #expect(await source.snapshot.messages.map(\.messageId) == ["m1"])
    }

    /// A room whose product does not answer keeps its cached page and is unavailable.
    @Test func anUnavailableRoomKeepsItsCachedPage() async {
        let cache = ProductMessageCache()
        await cache.store(
            ProductRoomCacheEntry(
                messages: [productMessage(1), productMessage(2)],
                next: "1",
                seenMessageId: "m2",
                seenTimestamp: 2_000,
                reportedSeenMessageId: nil
            ),
            productId: productId,
            roomId: roomId
        )
        let querying = ScriptedProductChatQuerying(page: { _ in throw ChatQueryError.noAnswer })
        let source = makeSource(querying, cache: cache)
        await open(source)

        let snapshot = await source.snapshot
        #expect(snapshot.readiness == .unavailable)
        #expect(snapshot.messages.map(\.messageId) == ["m1", "m2"])
        #expect(querying.pageRequests.count == 2)
    }

    @Test func theCacheKeepsTheNewestPageForTheNextOpen() async {
        let cache = ProductMessageCache()
        let history = ProductRoomHistory(count: 80)
        let first = makeSource(ScriptedProductChatQuerying(page: history.answer), cache: cache)
        await open(first)

        let second = makeSource(ScriptedProductChatQuerying(page: { _ in throw ChatQueryError.noAnswer }), cache: cache)
        await second.start()

        #expect(await second.snapshot.messages.map(\.messageId) == history.ids(30 ..< 80))
    }

    // MARK: - Feed updates

    /// New (in timestamp order), edit, reactions and status change apply to the window.
    @Test func feedUpdatesApplyToTheWindow() async throws {
        let history = ProductRoomHistory(count: 10)
        let source = makeSource(ScriptedProductChatQuerying(page: history.answer))
        await open(source)

        await source.apply(.new(productMessage(10)))
        await source.apply(.new(productMessage(45, id: "late", timestamp: 4_500)))
        await source.apply(.edited(messageId: "m3", body: .text(text: "edited"), editedAt: 99))
        await source.apply(.reactionPlaced(messageId: "m1", participantId: "a", emoji: "👍"))
        await source.apply(.reactionPlaced(messageId: "m1", participantId: "b", emoji: "👍"))
        await source.apply(.reactionRemoved(messageId: "m1", participantId: "b", emoji: "👍"))
        await source.apply(.reactionPlaced(messageId: "m1", participantId: "user", emoji: "🔥"))
        await source.apply(.new(productMessage(11, origin: "user")))
        await source.apply(.statusChanged(messageId: "m11", status: .read))

        let snapshot = await source.snapshot
        #expect(snapshot.messages.map(\.messageId) == [
            "m0", "m1", "m2", "m3", "m4", "late", "m5", "m6", "m7", "m8", "m9", "m10", "m11"
        ])

        let edited = try #require(snapshot.messages.first { $0.messageId == "m3" })
        #expect(edited.content == .text("edited"))
        #expect(snapshot.editedAt == ["m3": 99])

        let reacted = try #require(snapshot.messages.first { $0.messageId == "m1" })
        #expect(reacted.reactions.map(\.emoji).sorted() == ["👍", "🔥"])
        #expect(reacted.reactions.first { $0.emoji == "🔥" }?.origin == .user)

        let sent = try #require(snapshot.messages.last)
        // The native chat draws a read message as delivered.
        #expect(sent.status == .outgoing(.delivered))
        #expect(sent.origin == .user)
    }
}

// MARK: - Helpers

private let productId = "product.dot"
private let roomId = "lobby"

extension ProductMessageSourceTests {
    /// A reply whose quoted message is outside the window quotes the message that the product gave with it.
    @Test func aReplyQuotesTheMessageThatTheProductGaveWithIt() async {
        var reply = productMessage(5)
        reply.replyTo = "m1"
        reply.replyPreview = ChatReplyPreview(
            origin: ChatMessageOrigin(participantId: "user"),
            body: .text(text: "quoted")
        )
        let page = ProductChatMessagesQueryItem(messages: [reply], next: "5")
        let querying = ScriptedProductChatQuerying(page: { _ in page })
        let source = makeSource(querying)
        await open(source)

        let quoted = await source.snapshot.replyPreviews["m1"]
        #expect(quoted?.content == .text("quoted"))
        #expect(quoted?.status.isIncoming == false)
    }
}

private func makeSource(
    _ querying: ScriptedProductChatQuerying,
    cache: ProductMessageCache = ProductMessageCache()
) -> ProductMessageSource {
    ProductMessageSource(
        chatId: .chatExtension(productId, roomId: roomId),
        productId: productId,
        roomId: roomId,
        querying: querying,
        cache: cache,
        settings: .init(retryDelay: .milliseconds(1)),
        logger: StubLogger()
    )
}

private func open(_ source: ProductMessageSource) async {
    await source.start()
    await source.waitForFirstPage()
}

private func request(_ anchor: ChatPageAnchor) -> ProductChatMessagesQueryRequest {
    ProductChatMessagesQueryRequest(roomId: roomId, anchor: anchor, pageSize: 50)
}

private func productMessage(
    _ index: Int,
    id: String? = nil,
    origin: String = "product",
    timestamp: UInt64? = nil,
    status: ChatMessageStatus = .sent
) -> ChatMessage {
    ChatMessage(
        messageId: id ?? "m\(index)",
        origin: ChatMessageOrigin(participantId: origin),
        timestamp: timestamp ?? UInt64(index) * 1_000,
        body: .text(text: "message \(index)"),
        status: status,
        replyTo: nil,
        replyPreview: nil,
        editedAt: nil,
        reactions: [],
        request: false
    )
}

/// A room of the product with slot cursors: a `Before` cursor is the slot where the page ends.
private final class ProductRoomHistory: Sendable {
    private let count: OSAllocatedUnfairLock<Int>
    private let origin: String

    init(count: Int, origin: String = "product") {
        self.count = OSAllocatedUnfairLock(initialState: count)
        self.origin = origin
    }

    func append(_ added: Int) {
        count.withLock { $0 += added }
    }

    func ids(_ range: Range<Int>) -> [String] {
        range.map { "m\($0)" }
    }

    var answer: ScriptedProductChatQuerying.PageAnswer {
        { [self] request in page(request) }
    }

    private func page(_ request: ProductChatMessagesQueryRequest) -> ProductChatMessagesQueryItem {
        let total = count.withLock { $0 }
        let size = Int(request.pageSize)

        switch request.anchor {
        case let .before(cursor):
            let end = cursor.flatMap(Int.init) ?? total
            let start = max(end - size, 0)
            return ProductChatMessagesQueryItem(
                messages: (start ..< end).map { productMessage($0, origin: origin) },
                next: start > 0 ? String(start) : nil
            )
        case let .after(cursor):
            let start = cursor.flatMap(Int.init) ?? 0
            let end = min(start + size, total)
            return ProductChatMessagesQueryItem(
                messages: (start ..< end).map { productMessage($0, origin: origin) },
                next: end < total ? String(end) : nil
            )
        }
    }
}
