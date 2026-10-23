import Foundation
import Testing
@testable import TrUAPIHost

struct ChatHostBridgeDefaultsTests {
    /// A product may declare the configuration and metadata that a host
    /// without the v2 requests serves anyway, so that host must accept them.
    @Test
    func defaultsAcceptTheDeclarationsOfANewRoom() async throws {
        let bridge: ChatHostBridge = FirstVersionChatHostBridge()

        try await bridge.setRoomConfig(roomId: "room", config: .hostDefault)
        try await bridge.setRoomMetadata(
            roomId: "room",
            metadata: ChatRoomMetadata(name: "Room", description: nil)
        )

        #expect(try await bridge.roomConfig(roomId: "room") == .hostDefault)
    }

    /// A declaration that the host cannot serve must fail, not disappear.
    @Test
    func defaultsRefuseWhatTheHostDoesNotServe() async throws {
        let bridge: ChatHostBridge = FirstVersionChatHostBridge()
        var config = ChatRoomConfig.hostDefault
        config.productSource = true

        await #expect(throws: HostRejection.self) {
            try await bridge.setRoomConfig(roomId: "room", config: config)
        }
        await #expect(throws: HostRejection.self) {
            try await bridge.deleteRoom(roomId: "room")
        }
        await #expect(throws: NativeChatFeedRejection.self) {
            try await bridge.updateFeed(
                roomId: "room",
                update: .statusChanged(messageId: "message", status: .read),
                persist: true
            )
        }
    }
}

struct ChatAdditionsTests {
    /// A host that does not serve the additions lists no feature and reads
    /// default declarations, so that a product falls back to what every v2
    /// host serves.
    @Test
    func defaultsListNoFeatureAndReadDefaults() async throws {
        let bridge: ChatHostBridge = FirstVersionChatHostBridge()

        #expect(try await bridge.capabilities().isEmpty)
        #expect(try await bridge.roomFeatures(roomId: "room") == .hostDefault)
    }

    /// A change that the host cannot keep must fail, not disappear.
    @Test
    func defaultsRefuseTheChangesOfTheAdditions() async {
        let bridge: ChatHostBridge = FirstVersionChatHostBridge()

        await #expect(throws: HostRejection.self) {
            try await bridge.setRoomFeatures(roomId: "room", features: .hostDefault)
        }
        await #expect(throws: HostRejection.self) {
            try await bridge.setRoomCall(roomId: "room", call: nil)
        }
        await #expect(throws: HostRejection.self) {
            try await bridge.setRoomContact(
                roomId: "room",
                contact: ChatRoomContact(account: Data(count: 32), username: "alice", state: .incoming)
            )
        }
    }

    @Test
    func editHistoryAnswerReachesTheCaller() async throws {
        let history = ProductChatEditHistoryQueryItem(revisions: [
            ChatMessageRevision(editedAt: 1, text: "first"),
            ChatMessageRevision(editedAt: 2, text: nil),
        ])

        let answer = try await chatQuery(timeout: ChatQueryTimeout.lookup) { observer in
            observer.onAnswer(answer: .editHistory(history: history))
            return RecordingChatQuery()
        } answer: { answer in
            if case let .editHistory(history) = answer { history } else { nil }
        }

        #expect(answer == history)
    }

    /// The core screens a room event before the product sees it, and keeps
    /// a valid one for a product that has no worker yet.
    @Test
    func roomEventIsScreenedAndKept() throws {
        let runtime = try TrUAPIHostRuntime(
            bridge: StubHostBridge(),
            runtimeConfig: TrUAPIWsBridgeTests.makeHostRuntimeConfig()
        )

        try runtime.publishChatRoomEvent(
            productId: "test.dot",
            roomId: "room",
            event: .callRequested(video: true, callId: "c1")
        )
        #expect(throws: HostRejection.self) {
            try runtime.publishChatRoomEvent(
                productId: "test.dot",
                roomId: "room",
                event: .callSignal(callId: "call", signal: Data())
            )
        }
    }

    @Test
    func wakeHoldsTheWorkerAndHopStagingNeedsADeclaredRoom() async throws {
        let runtime = try TrUAPIHostRuntime(
            bridge: StubHostBridge(),
            runtimeConfig: TrUAPIWsBridgeTests.makeHostRuntimeConfig()
        )

        let outcome = try runtime.wakeProduct(productId: "test.dot", reason: .productStart)
        guard case .held = outcome else {
            Issue.record("a first wake holds the worker, not \(outcome)")
            return
        }
        await #expect(throws: NativeChatStageRejection.self) {
            try await runtime.hopChatAttachment(
                productId: "test.dot",
                roomId: "room",
                bytes: Data([1])
            )
        }
    }
}

struct ChatQueryTests {
    private static let page = ProductChatMessagesQueryItem(messages: [], next: nil)

    @Test
    func answerReachesTheCallerAndEndsTheQuery() async throws {
        let query = RecordingChatQuery()

        let page = try await chatQuery(timeout: .seconds(5)) { observer in
            observer.onAnswer(answer: .messages(page: Self.page))
            return query
        } answer: { answer in
            if case let .messages(page) = answer { page } else { nil }
        }

        #expect(page == Self.page)
        #expect(query.cancelCount == 1)
    }

    @Test
    func refusalBecomesAChatQueryError() async {
        await #expect(throws: ChatQueryError.refused(.notAvailable)) {
            try await chatQuery(timeout: .seconds(5)) { observer in
                observer.onRefusal(refusal: .query(error: .notAvailable))
                return RecordingChatQuery()
            } answer: { _ in () }
        }
        await #expect(throws: ChatQueryError.actionRefused(.rejected(reason: "no"))) {
            try await chatQuery(timeout: .seconds(5)) { observer in
                observer.onRefusal(refusal: .userAction(error: .rejected(reason: "no")))
                return RecordingChatQuery()
            } answer: { _ in () }
        }
    }

    @Test
    func endWithoutAnswerIsNoAnswer() async {
        await #expect(throws: ChatQueryError.noAnswer) {
            try await chatQuery(timeout: .seconds(5)) { observer in
                observer.onComplete()
                return RecordingChatQuery()
            } answer: { _ in () }
        }
    }

    @Test
    func answerOfAnotherQueryFails() async {
        await #expect(throws: ChatQueryError.failed(reason: "the product answered another query")) {
            try await chatQuery(timeout: .seconds(5)) { observer in
                observer.onAnswer(answer: .messages(page: Self.page))
                return RecordingChatQuery()
            } answer: { answer -> ProductChatReactionsQueryItem? in
                if case let .reactions(reactions) = answer { reactions } else { nil }
            }
        }
    }

    @Test(.timeLimit(.minutes(1)))
    func silentProductTimesOutAndTheQueryStops() async {
        let query = RecordingChatQuery()

        await #expect(throws: ChatQueryError.timedOut) {
            try await chatQuery(timeout: .milliseconds(50)) { _ in query } answer: { _ in () }
        }
        #expect(query.cancelCount == 1)
    }

    @Test(.timeLimit(.minutes(1)))
    func cancelledTaskStopsTheQuery() async {
        let query = RecordingChatQuery()
        let started = AsyncStream<Void>.makeStream()
        let task = Task {
            try await chatQuery(timeout: .seconds(60)) { _ in
                started.continuation.yield()
                return query
            } answer: { _ in () }
        }
        for await _ in started.stream { break }

        task.cancel()

        await #expect(throws: CancellationError.self) { try await task.value }
        #expect(query.cancelCount == 1)
    }

    @Test
    func userActionTimeoutFollowsTheAction() {
        let sent = ChatUserAction.newMessage(
            body: .text(text: "hi"),
            replyTo: nil,
            messageId: nil,
            timestamp: nil
        )
        let paid = ChatUserAction.sendPayment(
            messageId: "payment",
            timestamp: 1_700_000_000_000,
            totalValue: "5000000",
            coinKeys: [Data(repeating: 0x11, count: 64)]
        )
        let seen = ChatUserAction.seen(messageId: "message")

        #expect(ChatQueryTimeout.timeout(for: sent) == .seconds(30))
        #expect(ChatQueryTimeout.timeout(for: paid) == .seconds(30))
        #expect(ChatQueryTimeout.timeout(for: seen) == .seconds(10))
    }

    /// Without a signed-in session no product runs, so a query fails at once
    /// and does not wait for its timeout.
    @Test(.timeLimit(.minutes(1)))
    func queryWithoutSessionFailsAtOnce() async throws {
        let bridge = StubHostBridge()
        let runtime = try TrUAPIHostRuntime(
            bridge: bridge,
            runtimeConfig: TrUAPIWsBridgeTests.makeHostRuntimeConfig()
        )
        let execution = try runtime.openProductExecution(
            bridge: bridge,
            configuration: ProductExecutionConfig(productId: "test.dot", executionKind: .worker),
            chat: StubChatHostBridge()
        )
        let request = ProductChatMessagesQueryRequest(
            roomId: "room",
            anchor: .before(cursor: nil),
            pageSize: 40
        )

        await #expect(throws: ProductRuntimeError.self) {
            try await execution.chatMessagesQuery(request)
        }
        #expect(throws: ProductRuntimeError.self) {
            try execution.publishChatAction(HostChatActionSubscribeItem(
                roomId: "room",
                peer: "native",
                payload: .messagePosted(.text(text: "hi"))
            ))
        }
    }
}

/// A host that implements only the requests of the first chat version.
private final class FirstVersionChatHostBridge: ChatHostBridge {
    func createRoom(
        roomId _: String,
        name _: String,
        icon _: String
    ) async throws -> ChatRoomRegistrationStatus { .new }

    func registerBot(
        botId _: String,
        name _: String,
        icon _: String
    ) async throws -> ChatBotRegistrationStatus { .new }

    func postMessage(roomId _: String, content _: ChatMessageContent, alt _: String?) async throws -> String {
        "message-id"
    }

    func setRoomFooter(roomId _: String, footer _: ChatRoomFooter) async throws {}

    func listRooms() async throws -> [ChatRoom] { [] }
}

/// A query handle without a product, which counts its cancellations.
private final class RecordingChatQuery: NativeChatQuery, @unchecked Sendable {
    private let lock = NSLock()
    private var cancellations = 0

    var cancelCount: Int { lock.withLock { cancellations } }

    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func cancel() {
        lock.withLock { cancellations += 1 }
    }
}
