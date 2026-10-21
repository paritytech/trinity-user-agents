import Foundation
import os
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The search of the chat list asks the product-source rooms whose worker runs (VERBOSITY-WASM-PRODUCT C11).
struct ChatListProductSearchTests {
    private let productId = "chat.dot"

    @Test func eachRoomGivesItsHitsNewestFirst() async {
        let product = ScriptedListSearch { request in
            .init(hits: [hit("\(request.roomId)-old", at: 1_000), hit("\(request.roomId)-new", at: 3_000)], next: nil)
        }
        let search = makeSearch(product)

        let hits = await search.search("hello", in: [room("a"), room("b")])

        #expect(hits.map(\.timestamp) == [3_000, 3_000, 1_000, 1_000])
        #expect(Set(hits.map(\.chatId)) == [room("a"), room("b")])
        #expect(product.requests.map(\.text) == ["hello", "hello"])
        #expect(product.requests.map(\.pageSize) == [ChatListProductSearch.pageSize, ChatListProductSearch.pageSize])
    }

    @Test func aProductThatDoesNotRunGivesNoHits() async {
        let search = makeSearch(ScriptedListSearch { _ in nil })

        #expect(await search.search("hello", in: [room("a")]).isEmpty)
    }

    @Test func aFailedRoomDoesNotHideTheOthers() async {
        let product = ScriptedListSearch { request in
            guard request.roomId == "b" else { throw ChatQueryError.timedOut }
            return .init(hits: [hit("m1", at: 1)], next: nil)
        }

        let hits = await makeSearch(product).search("hello", in: [room("a"), room("b")])

        #expect(hits.map(\.chatId) == [room("b")])
    }

    @Test func aSlowRoomEndsAtTheDeadline() async {
        let product = ScriptedListSearch { request in
            if request.roomId == "slow" {
                try await Task.sleep(for: .seconds(30))
            }
            return .init(hits: [hit("m1", at: 1)], next: nil)
        }
        let search = makeSearch(product, deadline: .milliseconds(200))
        let clock = ContinuousClock()

        let start = clock.now
        let hits = await search.search("hello", in: [room("fast"), room("slow")])

        #expect(clock.now - start < .seconds(5))
        #expect(hits.map(\.chatId) == [room("fast")])
    }

    @Test func onlyTheNewestRoomsAreAsked() async {
        let product = ScriptedListSearch { _ in .init(hits: [], next: nil) }
        let rooms = (0 ..< 12).map { room("r\($0)") }

        _ = await makeSearch(product, roomLimit: 3).search("hello", in: rooms)

        #expect(Set(product.requests.map(\.roomId)) == ["r0", "r1", "r2"])
    }

    @Test func aPersonChatIsNotAsked() async {
        let product = ScriptedListSearch { _ in .init(hits: [hit("m1", at: 1)], next: nil) }

        let hits = await makeSearch(product).search("hello", in: [.person(Data(repeating: 1, count: 32))])

        #expect(hits.isEmpty)
        #expect(product.requests.isEmpty)
    }

    /// A hit of the list opens a room whose window holds only the newest page: the jump starts the source and pages
    /// back to the message.
    @Test func aJumpBeforeTheRoomStartsPagesToTheMessage() async {
        let querying = ScriptedProductChatQuerying(page: { request in
            let older = request.anchor == .before(cursor: "50")
            let range = older ? 0 ..< 50 : 50 ..< 100
            return .init(messages: range.map { hit("m\($0)", at: UInt64($0) * 1_000) }, next: older ? nil : "50")
        })
        let source = ProductMessageSource(
            chatId: room("dm-1"),
            productId: productId,
            roomId: "dm-1",
            querying: querying,
            cache: ProductMessageCache(),
            settings: .init(retryDelay: .milliseconds(1)),
            logger: StubLogger()
        )

        #expect(await source.jump(to: "m3"))
        #expect(await source.snapshot.messages.contains { $0.messageId == "m3" })
    }
}

private extension ChatListProductSearchTests {
    func room(_ roomId: String) -> Chat.Id {
        .chatExtension(productId, roomId: roomId)
    }

    func makeSearch(
        _ product: ScriptedListSearch,
        roomLimit: Int = ChatListProductSearch.roomLimit,
        deadline: Duration = .seconds(5)
    ) -> ChatListProductSearch {
        ChatListProductSearch(
            searching: { _ in product },
            roomLimit: roomLimit,
            deadline: deadline,
            logger: StubLogger()
        )
    }

    func hit(_ messageId: String, at timestamp: UInt64) -> ChatMessage {
        ChatMessage(
            messageId: messageId,
            origin: ChatMessageOrigin(participantId: "c1"),
            timestamp: timestamp,
            body: .text(text: "hello there"),
            status: .delivered,
            replyTo: nil,
            editedAt: nil,
            reactions: []
        )
    }
}

private final class ScriptedListSearch: ProductChatListSearching, @unchecked Sendable {
    typealias Answer = @Sendable (ProductChatSearchQueryRequest) async throws -> ProductChatSearchQueryItem?

    private let answer: Answer
    private let recorded = OSAllocatedUnfairLock<[ProductChatSearchQueryRequest]>(initialState: [])

    init(_ answer: @escaping Answer) {
        self.answer = answer
    }

    var requests: [ProductChatSearchQueryRequest] {
        recorded.withLock { $0 }
    }

    func searchPageIfRunning(_ request: ProductChatSearchQueryRequest) async throws -> ProductChatSearchQueryItem? {
        recorded.withLock { $0.append(request) }
        return try await answer(request)
    }
}
