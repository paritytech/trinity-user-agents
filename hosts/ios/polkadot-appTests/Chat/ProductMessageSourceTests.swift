import Foundation
import os
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The window of a product-source room (04-TESTING S15 to S22, P5.2).
struct ProductMessageSourceTests {
    // MARK: - Pages

    /// S15: the room opens on `Before { None }` with the newest message at the bottom.
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

    /// S16: scrolling up pages with the returned cursor until `next: None`, and asks no page twice.
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

    /// S22: the window never holds more than 500 messages, and a dropped page comes back with its request.
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

    /// S17: a jump pages until the message is in the window, and fails only when both ends come first.
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

    /// 02-DESIGN §5.1: the host retries an `unavailable` interrupt once after a delay.
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

    /// S23 (source part): a room whose product does not answer keeps its cached page and is unavailable.
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

    /// S18: new (in timestamp order), edit, delete, reactions and status change apply to the window.
    @Test func feedUpdatesApplyToTheWindow() async throws {
        let history = ProductRoomHistory(count: 10)
        let source = makeSource(ScriptedProductChatQuerying(page: history.answer))
        await open(source)

        await source.apply(.new(productMessage(10)))
        await source.apply(.new(productMessage(45, id: "late", timestamp: 4_500)))
        await source.apply(.edited(messageId: "m3", body: .text(text: "edited"), editedAt: 99))
        await source.apply(.deleted(messageId: "m2"))
        await source.apply(.reactionPlaced(messageId: "m1", participantId: "a", emoji: "👍"))
        await source.apply(.reactionPlaced(messageId: "m1", participantId: "b", emoji: "👍"))
        await source.apply(.reactionRemoved(messageId: "m1", participantId: "b", emoji: "👍"))
        await source.apply(.reactionPlaced(messageId: "m1", participantId: "user", emoji: "🔥"))
        await source.apply(.new(productMessage(11, origin: "user")))
        await source.apply(.statusChanged(messageId: "m11", status: .read))

        let snapshot = await source.snapshot
        #expect(snapshot.messages.map(\.messageId) == [
            "m0", "m1", "m3", "m4", "late", "m5", "m6", "m7", "m8", "m9", "m10", "m11"
        ])

        let edited = try #require(snapshot.messages.first { $0.messageId == "m3" })
        #expect(edited.content == .text("edited"))
        #expect(snapshot.editedAt == ["m3": 99])

        let reacted = try #require(snapshot.messages.first { $0.messageId == "m1" })
        #expect(reacted.reactions.map(\.emoji).sorted() == ["👍", "🔥"])
        #expect(reacted.reactions.first { $0.emoji == "🔥" }?.origin == .user)

        let sent = try #require(snapshot.messages.last)
        #expect(sent.status == .outgoing(.read))
        #expect(sent.origin == .user)
    }

    /// A message that arrives live after the user saw the room counts as unread until the user sees it.
    @Test func aLiveMessageIsUnreadUntilSeen() async throws {
        let history = ProductRoomHistory(count: 3)
        let source = makeSource(ScriptedProductChatQuerying(page: history.answer))
        await open(source)

        await source.apply(.new(productMessage(3)))
        #expect(await source.snapshot.messages.map(\.status) == [
            .incoming(.seen), .incoming(.seen), .incoming(.seen), .incoming(.new)
        ])

        _ = try await source.perform(.markSeen(["m3"]))
        #expect(await source.snapshot.messages.last?.status == .incoming(.seen))
    }

    // MARK: - User actions

    /// S19: a send shows `Preparing`, then the message of `MessageCreated` replaces it.
    @Test func aSendIsOptimisticUntilTheProductCreatesTheMessage() async throws {
        let gate = AnswerGate()
        let created = productMessage(50, id: "created", origin: "user", status: .sent)
        let querying = ScriptedProductChatQuerying(action: { _ in
            await gate.wait()
            return .messageCreated(created)
        })
        let source = makeSource(querying)
        await open(source)

        let send = Task { try await source.perform(.send(.text("hi"))) }
        let optimistic = try #require(await firstLocalMessage(in: source))
        #expect(optimistic.status == .outgoing(.new))
        #expect(optimistic.content == .text("hi"))

        await gate.open()
        #expect(try await send.value == .done)

        let snapshot = await source.snapshot
        #expect(!snapshot.messages.contains { $0.messageId.hasPrefix("local:") })
        #expect(snapshot.messages.last?.messageId == "created")
        #expect(snapshot.messages.last?.status == .outgoing(.sent))
        #expect(querying.actionRequests.map(\.action) == [.newMessage(body: .text(text: "hi"), replyTo: nil)])
    }

    @Test func aReplyCarriesTheMessageItAnswers() async throws {
        let querying = ScriptedProductChatQuerying(action: { _ in .acknowledged })
        let source = makeSource(querying)
        await open(source)

        let reply = Chat.RemoteMessageContentV1.MessageContent.ReplyContent(
            messageId: "m1",
            ownContent: .init(text: "yes", attachments: nil)
        )
        _ = try await source.perform(.send(.reply(reply)))

        #expect(querying.actionRequests.map(\.action) == [.newMessage(body: .text(text: "yes"), replyTo: "m1")])
        #expect(await source.snapshot.messages.last?.content == .reply(reply))
    }

    /// S20: a send that the product rejects goes away and gives the reason.
    @Test func aRejectedSendGoesAwayWithTheReason() async throws {
        let querying = ScriptedProductChatQuerying(action: { _ in
            throw ChatQueryError.actionRefused(.rejected(reason: "Too long"))
        })
        let source = makeSource(querying)
        await open(source)

        #expect(try await source.perform(.send(.text("hi"))) == .refused(reason: "Too long"))
        #expect(await source.snapshot.messages.isEmpty)
    }

    /// S21: no answer in time marks the send failed, and a retry sends it again.
    @Test func aTimedOutSendFailsAndARetrySendsItAgain() async throws {
        let attempts = OSAllocatedUnfairLock(initialState: 0)
        let created = productMessage(60, id: "created", origin: "user", status: .sent)
        let querying = ScriptedProductChatQuerying(action: { _ in
            let attempt = attempts.withLock { count in
                count += 1
                return count
            }
            guard attempt > 1 else {
                throw ChatQueryError.timedOut
            }
            return .messageCreated(created)
        })
        let source = makeSource(querying)
        await open(source)

        #expect(try await source.perform(.send(.text("hi"))) == .failed)
        let failed = try #require(await source.snapshot.messages.last)
        #expect(failed.status == .outgoing(.failed))
        #expect(await source.snapshot.readiness == .ready)

        #expect(try await source.perform(.retry(failed.messageId)) == .done)
        #expect(await source.snapshot.messages.map(\.messageId) == ["created"])
        #expect(querying.actionRequests.map(\.action) == [
            .newMessage(body: .text(text: "hi"), replyTo: nil),
            .newMessage(body: .text(text: "hi"), replyTo: nil)
        ])
    }

    @Test func aRetryOfAFailedProductMessageAsksTheProduct() async throws {
        let querying = ScriptedProductChatQuerying(action: { _ in .acknowledged })
        let source = makeSource(querying)
        await open(source)
        await source.apply(.new(productMessage(1, origin: "user", status: .failed)))

        #expect(try await source.perform(.retry("m1")) == .done)

        #expect(querying.actionRequests.map(\.action) == [.retry(messageId: "m1")])
        #expect(await source.snapshot.messages.last?.status == .outgoing(.new))
    }

    @Test func aRefusedEditRestoresTheBody() async throws {
        let history = ProductRoomHistory(count: 5, origin: "user")
        let querying = ScriptedProductChatQuerying(page: history.answer, action: { _ in
            throw ChatQueryError.actionRefused(.rejected(reason: "Locked"))
        })
        let source = makeSource(querying)
        await open(source)

        let edit = Chat.RemoteMessageContentV1.MessageContent.EditedContent(
            messageId: "m3",
            newContent: .init(text: "new", attachments: nil)
        )
        #expect(try await source.perform(.send(.edited(edit))) == .refused(reason: "Locked"))

        #expect(querying.actionRequests.map(\.action) == [.editMessage(messageId: "m3", body: .text(text: "new"))])
        #expect(await source.snapshot.messages.first { $0.messageId == "m3" }?.content == .text("message 3"))
    }

    @Test func aDeleteThatTimesOutPutsTheMessageBack() async throws {
        let history = ProductRoomHistory(count: 3)
        let querying = ScriptedProductChatQuerying(page: history.answer, action: { _ in
            throw ChatQueryError.timedOut
        })
        let source = makeSource(querying)
        await open(source)

        #expect(try await source.perform(.delete("m1")) == .refused(reason: nil))
        await waitUntil { await source.snapshot.messages.count == 3 }

        #expect(await source.snapshot.messages.map(\.messageId) == ["m0", "m1", "m2"])
    }

    @Test func aReactionTogglesTheReactionOfTheUser() async throws {
        let history = ProductRoomHistory(count: 2)
        let querying = ScriptedProductChatQuerying(page: history.answer)
        let source = makeSource(querying)
        await open(source)

        _ = try await source.perform(.toggleReaction(messageId: "m1", emoji: "👍"))
        #expect(await source.snapshot.messages.last?.reactions.map(\.origin) == [.user])

        _ = try await source.perform(.toggleReaction(messageId: "m1", emoji: "👍"))
        #expect(await source.snapshot.messages.last?.reactions.isEmpty == true)

        #expect(querying.actionRequests.map(\.action) == [
            .reactionPlaced(messageId: "m1", emoji: "👍"),
            .reactionRemoved(messageId: "m1", emoji: "👍")
        ])
    }

    /// 02-DESIGN §5.3 step 5: `Seen` goes to the product only when it asks, and only for a newer message.
    @Test func seenGoesToTheProductForANewerMessage() async throws {
        let history = ProductRoomHistory(count: 10)
        let querying = ScriptedProductChatQuerying(page: history.answer)
        let source = makeSource(querying, reportsSeen: true)
        await open(source)

        _ = try await source.perform(.markSeen(["m4", "m9"]))
        await waitUntil { querying.actionRequests.count == 1 }
        _ = try await source.perform(.markSeen(["m5"]))
        _ = try await source.perform(.markSeen(["m9"]))

        #expect(querying.actionRequests.map(\.action) == [.seen(messageId: "m9")])
    }

    @Test func seenStaysInTheHostWhenTheProductDoesNotAsk() async throws {
        let history = ProductRoomHistory(count: 10)
        let querying = ScriptedProductChatQuerying(page: history.answer)
        let source = makeSource(querying, reportsSeen: false)
        await open(source)

        _ = try await source.perform(.markSeen(["m9"]))

        #expect(querying.actionRequests.isEmpty)
    }

    @Test func aFeedUpdateForAnotherRoomIsIgnored() async {
        let source = makeSource(ScriptedProductChatQuerying())
        await open(source)

        await source.apply(roomId: "other", update: .new(productMessage(1)))
        await source.apply(roomId: roomId, update: .new(productMessage(2)))

        #expect(await source.snapshot.messages.map(\.messageId) == ["m2"])
    }
}

// MARK: - Helpers

private let productId = "product.dot"
private let roomId = "lobby"

extension ProductMessageSourceTests {
    /// P7.3: the worker ends while a send waits for its answer (06-OPEN-ITEMS O26). The send fails, the room is
    /// unavailable, and the window keeps the messages that it has.
    @Test func aSendWhoseWorkerEndsFailsAndTheRoomKeepsItsMessages() async throws {
        let querying = ScriptedProductChatQuerying(
            page: { _ in .init(messages: [productMessage(10, id: "p1")], next: nil) },
            action: { _ in throw ChatQueryError.noAnswer }
        )
        let source = makeSource(querying)
        await open(source)

        #expect(try await source.perform(.send(.text("hi"))) == .failed)
        await source.productStopped()

        let snapshot = await source.snapshot
        #expect(snapshot.readiness == .unavailable)
        #expect(snapshot.messages.first?.messageId == "p1")
        #expect(snapshot.messages.last?.status == .outgoing(.failed))
    }
}

private func makeSource(
    _ querying: ScriptedProductChatQuerying,
    cache: ProductMessageCache = ProductMessageCache(),
    reportsSeen: Bool = false
) -> ProductMessageSource {
    ProductMessageSource(
        chatId: .chatExtension(productId, roomId: roomId),
        productId: productId,
        roomId: roomId,
        querying: querying,
        cache: cache,
        settings: .init(retryDelay: .milliseconds(1), reportsSeen: reportsSeen),
        now: { 1_000_000 },
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
        editedAt: nil,
        reactions: []
    )
}

private func firstLocalMessage(in source: ProductMessageSource) async -> Chat.LocalMessage? {
    guard let snapshots = try? await source.snapshots() else {
        return nil
    }

    do {
        for try await snapshot in snapshots {
            if let local = snapshot.messages.first(where: { $0.messageId.hasPrefix("local:") }) {
                return local
            }
        }
    } catch {
        return nil
    }

    return nil
}

private func waitUntil(_ condition: @Sendable () async -> Bool) async {
    for _ in 0 ..< 500 where await !condition() {
        try? await Task.sleep(for: .milliseconds(2))
    }
}

/// A room of the product with slot cursors, as the sample product pages it: a `Before` cursor is the slot
/// where the page ends.
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

/// Holds the answers of a scripted product until the test opens it.
private actor AnswerGate {
    private var isOpen = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func wait() async {
        guard !isOpen else {
            return
        }

        await withCheckedContinuation { waiters.append($0) }
    }

    func open() {
        isOpen = true
        waiters.forEach { $0.resume() }
        waiters = []
    }
}
