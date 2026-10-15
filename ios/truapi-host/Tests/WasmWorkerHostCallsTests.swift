import Foundation
import Testing
@testable import TrUAPIHost

/// The path of the example worker `rust/guests/chat-source`, built for
/// wasm32. With xcodebuild, set `TEST_RUNNER_TRUAPI_TEST_CHAT_SOURCE_WASM`.
private let chatSourceWasm = ProcessInfo.processInfo.environment["TRUAPI_TEST_CHAT_SOURCE_WASM"]

struct WasmWorkerHostCallsTests {
    /// The app asks the execution of a worker for the messages of a room
    /// that the product keeps, sends what the user writes, and draws a custom
    /// body. A wasm worker answers these calls through the same execution
    /// methods as a script worker, so the app needs no wasm code for them.
    @Test(
        .enabled(if: chatSourceWasm != nil, "set TRUAPI_TEST_CHAT_SOURCE_WASM"),
        .timeLimit(.minutes(1))
    )
    func aWasmWorkerAnswersTheChatQueriesAndTheRenderOfItsExecution() async throws {
        let wasm = try Data(contentsOf: URL(fileURLWithPath: try #require(chatSourceWasm)))
        let bridge = StubHostBridge()
        var config = try TrUAPIWsBridgeTests.makeHostRuntimeConfig()
        config.localSessionSecret = Data(repeating: 0xAB, count: 32)
        let runtime = try TrUAPIHostRuntime(bridge: bridge, runtimeConfig: config)
        let execution = try runtime.openProductExecution(
            bridge: bridge,
            configuration: ProductExecutionConfig(productId: "chat-source.dot", executionKind: .worker),
            chat: SourceRoomChatBridge()
        )
        defer { execution.close() }
        _ = try execution.startWasmWorker(wasm)
        try await waitUntil { bridge.coreLogs.contains("wasm-worker room notes ready") }
        let page = ProductChatMessagesQueryRequest(roomId: "notes", anchor: .before(cursor: nil), pageSize: 20)
        let hello = ProductChatUserActionRequest(
            roomId: "notes",
            action: .newMessage(body: .text(text: "hello"), replyTo: nil)
        )
        let card = ProductRendererRenderRequest(
            context: .chatMessage(roomId: "notes", messageId: "n0", messageType: "card"),
            payload: Data("welcome".utf8)
        )

        let first = try await execution.chatMessagesQuery(page)
        let created = try await execution.chatUserAction(hello)
        let second = try await execution.chatMessagesQuery(page)
        var render = try execution.render(card).makeAsyncIterator()
        let drawn = try await render.next()

        #expect(first.messages.map(Self.describe) == ["n0 <card>"])
        guard case let .messageCreated(message) = created else {
            Issue.record("the worker did not store the message: \(created)")
            return
        }
        #expect(Self.describe(message) == "n1 hello")
        #expect(second.messages.map(Self.describe) == ["n0 <card>", "n1 hello", "n2 noted: hello"])
        #expect(drawn == .string(text: "card: welcome"))
    }

    private static func describe(_ message: ChatMessage) -> String {
        switch message.body {
        case let .text(text):
            return "\(message.messageId) \(text)"
        case let .custom(messageType, _, _):
            return "\(message.messageId) <\(messageType)>"
        default:
            return "\(message.messageId) \(message.body)"
        }
    }

    private func waitUntil(_ condition: () -> Bool) async throws {
        let deadline = ContinuousClock.now + .seconds(30)
        while !condition() {
            try #require(ContinuousClock.now < deadline, "the worker did not start")
            try await Task.sleep(for: .milliseconds(20))
        }
    }
}

/// A chat host that keeps the configuration of each room, so that the core
/// sees which rooms the product keeps.
private final class SourceRoomChatBridge: ChatHostBridge, @unchecked Sendable {
    private let lock = NSLock()
    private var rooms: [String: ChatRoomConfig] = [:]

    func createRoom(roomId: String, name _: String, icon _: String) async throws -> ChatRoomRegistrationStatus {
        lock.withLock {
            if rooms[roomId] != nil { return .exists }
            rooms[roomId] = .hostDefault
            return .new
        }
    }

    func registerBot(botId _: String, name _: String, icon _: String) async throws -> ChatBotRegistrationStatus { .new }

    func postMessage(roomId _: String, content _: ChatMessageContent, alt _: String?) async throws -> String {
        "posted"
    }

    func setRoomFooter(roomId _: String, footer _: ChatRoomFooter) async throws {}

    func listRooms() async throws -> [ChatRoom] {
        lock.withLock { rooms.keys.map { ChatRoom(roomId: $0, participatingAs: .roomHost) } }
    }

    func roomConfig(roomId: String) async throws -> ChatRoomConfig {
        lock.withLock { rooms[roomId] ?? .hostDefault }
    }

    func setRoomConfig(roomId: String, config: ChatRoomConfig) async throws {
        lock.withLock { rooms[roomId] = config }
    }

    func setRoomMetadata(roomId _: String, metadata _: ChatRoomMetadata) async throws {}

    func setRoomPreview(roomId _: String, preview _: ChatRoomPreview) async throws {}

    func deleteRoom(roomId _: String) async throws {}

    func updateFeed(roomId _: String, update _: ChatFeedUpdate, persist _: Bool) async throws {}

    func setRoomToolbar(roomId _: String, actions _: [ChatToolbarAction]) async throws {}

    func setRoomSuggestions(roomId _: String, suggestions _: [ChatSuggestion]) async throws {}

    func setRoomCommands(roomId _: String, commands _: [ChatCommandSpec]) async throws {}

    func setRoomActivity(roomId _: String, activity _: ChatActivity?) async throws {}
}
