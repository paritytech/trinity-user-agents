import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// Feed updates for a product-source room that no room screen shows change the cached page.
struct ProductMessageCacheTests {
    private let productId = "product.dot"
    private let roomId = "dm-1"

    @Test func eachFeedUpdateChangesTheCachedPage() async {
        let cache = ProductMessageCache()
        await cache.store(entry([message(1), message(2), message(3)]), productId: productId, roomId: roomId)

        await cache.apply(.new(message(5)), productId: productId, roomId: roomId)
        await cache.apply(.new(message(4)), productId: productId, roomId: roomId)
        await cache.apply(
            .edited(messageId: "m1", body: .text(text: "changed"), editedAt: 9),
            productId: productId,
            roomId: roomId
        )
        await cache.apply(
            .reactionPlaced(messageId: "m3", participantId: "c1", emoji: "👍"),
            productId: productId,
            roomId: roomId
        )
        await cache.apply(.statusChanged(messageId: "m5", status: .read), productId: productId, roomId: roomId)

        let cached = await cache.entry(productId: productId, roomId: roomId)
        #expect(cached?.messages.map(\.messageId) == ["m1", "m2", "m3", "m4", "m5"])
        #expect(cached?.messages.first?.body == .text(text: "changed"))
        #expect(cached?.messages.first?.editedAt == 9)
        let reaction = ChatReactionSummary(emoji: "👍", count: 1, reactedByUser: false, unseen: true)
        #expect(cached?.messages[2].reactions == [reaction])
        #expect(cached?.messages.last?.status == .read)
        #expect(cached?.next == "0")
        #expect(cached?.seenMessageId == "m3")
    }

    @Test func aNewMessageWithAKnownIdReplacesIt() async {
        let cache = ProductMessageCache()
        await cache.store(entry([message(1), message(2)]), productId: productId, roomId: roomId)

        await cache.apply(.new(message(2, text: "again")), productId: productId, roomId: roomId)

        let cached = await cache.entry(productId: productId, roomId: roomId)
        #expect(cached?.messages.map(\.messageId) == ["m1", "m2"])
        #expect(cached?.messages.last?.body == .text(text: "again"))
    }

    @Test func aFullPageDropsItsOldestMessageAndItsCursor() async {
        let cache = ProductMessageCache(messageLimit: 2)
        await cache.store(entry([message(1), message(2)]), productId: productId, roomId: roomId)

        await cache.apply(.new(message(3)), productId: productId, roomId: roomId)

        let cached = await cache.entry(productId: productId, roomId: roomId)
        #expect(cached?.messages.map(\.messageId) == ["m2", "m3"])
        #expect(cached?.next == nil)
    }

    @Test func aRoomWithNoCachedPageGetsNone() async {
        let cache = ProductMessageCache()

        await cache.apply(.new(message(1)), productId: productId, roomId: roomId)

        #expect(await cache.entry(productId: productId, roomId: roomId) == nil)
    }

    /// The bridge gives an update that no open room takes to the cache, and an update that an open room takes only
    /// to the room.
    @Test func theBridgeGivesTheCacheTheUpdatesOfAClosedRoom() async throws {
        let fixture = ProductRoomFixture()
        let listeners = ProductFeedListeners()
        let cache = ProductMessageCache()
        let bridge = await fixture.bridge(feedListeners: listeners, pageCache: cache)
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: "lobby", name: "Lobby", icon: nil)
        await cache.store(entry([message(1)]), productId: fixture.productId, roomId: "lobby")

        try await bridge.updateFeed(roomId: "lobby", update: .new(message(2)), persist: false)

        let listener = CachedFeedListener()
        let registration = listeners.register(listener, for: fixture.chatId("lobby"))
        try await bridge.updateFeed(roomId: "lobby", update: .new(message(3)), persist: false)
        registration.cancel()

        let cached = await cache.entry(productId: fixture.productId, roomId: "lobby")
        #expect(cached?.messages.map(\.messageId) == ["m1", "m2"])
        #expect(await listener.updates.map(\.update) == [.new(message(3))])
    }

    /// A room that opens after the update shows it from the cache before the product answers.
    @Test func aRoomThatOpensAfterTheUpdateShowsIt() async {
        let cache = ProductMessageCache()
        await cache.store(entry([message(1)]), productId: productId, roomId: roomId)
        await cache.apply(.new(message(2)), productId: productId, roomId: roomId)

        let source = ProductMessageSource(
            chatId: .chatExtension(productId, roomId: roomId),
            productId: productId,
            roomId: roomId,
            querying: ScriptedProductChatQuerying(page: { _ in throw ChatQueryError.noAnswer }),
            cache: cache,
            settings: .init(retryDelay: .milliseconds(1)),
            logger: StubLogger()
        )
        await source.start()

        #expect(await source.snapshot.messages.map(\.messageId) == ["m1", "m2"])
        #expect(await source.snapshot.messages.last?.status == .incoming(.new))
    }

    private func entry(_ messages: [ChatMessage]) -> ProductRoomCacheEntry {
        ProductRoomCacheEntry(
            messages: messages,
            next: "0",
            seenMessageId: messages.last?.messageId,
            seenTimestamp: messages.last?.timestamp,
            reportedSeenMessageId: nil
        )
    }

    private func message(_ index: Int, text: String? = nil) -> ChatMessage {
        ChatMessage(
            messageId: "m\(index)",
            origin: ChatMessageOrigin(participantId: "c1"),
            timestamp: UInt64(index) * 1_000,
            body: .text(text: text ?? "message \(index)"),
            status: .delivered,
            replyTo: nil,
            replyPreview: nil,
            editedAt: nil,
            reactions: [],
            request: false
        )
    }
}

private actor CachedFeedListener: ProductFeedListening {
    private(set) var updates: [(roomId: String, update: ChatFeedUpdate)] = []

    func apply(roomId: String, update: ChatFeedUpdate) {
        updates.append((roomId, update))
    }
}
