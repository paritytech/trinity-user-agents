import Foundation
import Testing
@testable import TrUAPIHost

/// The path of the example worker `rust/guests/chat-echo`, built for wasm32.
/// With xcodebuild, set `TEST_RUNNER_TRUAPI_TEST_CHAT_ECHO_WASM`.
private let chatEchoWasm = ProcessInfo.processInfo.environment["TRUAPI_TEST_CHAT_ECHO_WASM"]

struct WasmWorkerPushWakeTests {
    /// The core gives the wake topic that a worker sets, with the key of the
    /// device, to the relay of the host, and tells the worker that a push
    /// reaches it. A push that the relay did not sign wakes nothing.
    @Test(
        .enabled(if: chatEchoWasm != nil, "set TRUAPI_TEST_CHAT_ECHO_WASM"),
        .timeLimit(.minutes(1))
    )
    func theRelayGetsTheTopicOfAWorkerAndAForgedPushWakesNothing() async throws {
        let wasm = try Data(contentsOf: URL(fileURLWithPath: #require(chatEchoWasm)))
        let bridge = StubHostBridge()
        var config = try TrUAPIWsBridgeTests.makeHostRuntimeConfig()
        config.localSessionSecret = Data(repeating: 0xAB, count: 32)
        let runtime = try TrUAPIHostRuntime(bridge: bridge, runtimeConfig: config)
        let relay = RecordingWakeRelay()
        try await runtime.setWakeRelay(relay)
        let chat = EchoChatBridge()
        let execution = try runtime.openProductExecution(
            bridge: bridge,
            configuration: ProductExecutionConfig(productId: "echo.dot", executionKind: .worker),
            chat: chat
        )
        defer { execution.close() }
        _ = try execution.startWasmWorker(wasm)
        try await waitUntil { bridge.coreLogs.contains("wasm-worker room echo ready") }

        try execution.publishChatEvent(
            roomId: "echo",
            peer: "native",
            event: .messagePosted(messageId: "m1", body: .text(text: "/wake ping"), replyTo: nil)
        )
        try await waitUntil { !chat.posted.isEmpty }

        #expect(chat.posted == ["echo: /wake ping -> Push"])
        let registration = try #require(relay.registrations.last)
        var topic = Data("/statement ping".utf8)
        topic.append(Data(repeating: 0, count: 32 - topic.count))
        #expect(registration.topics == [topic])
        #expect(registration.key.count == 32)

        let forged = """
        {"v":1,"topic":"0x\(Self.hex(topic))","statement":"0x\(String(repeating: "09", count: 32))",\
        "sentAt":\(UInt64(Date().timeIntervalSince1970 * 1_000)),"mac":"0x\(String(repeating: "00", count: 32))"}
        """
        await #expect(throws: PushWakeRejection.BadSignature) {
            try await runtime.handleWakePush(payload: forged)
        }
        await #expect(throws: PushWakeRejection.self) {
            try await runtime.handleWakePush(payload: "not json")
        }
    }

    private static func hex(_ data: Data) -> String {
        data.map { String(format: "%02x", $0) }.joined()
    }

    private func waitUntil(_ condition: () -> Bool) async throws {
        let deadline = ContinuousClock.now + .seconds(30)
        while !condition() {
            try #require(ContinuousClock.now < deadline, "the worker did not get there")
            try await Task.sleep(for: .milliseconds(20))
        }
    }
}

/// A relay client that keeps each registration.
private final class RecordingWakeRelay: WakeRelayHostBridge, @unchecked Sendable {
    private let lock = NSLock()
    private var kept: [WakeRelayRegistration] = []

    var registrations: [WakeRelayRegistration] {
        lock.withLock { kept }
    }

    func registerWakeTopics(registration: WakeRelayRegistration) async throws {
        lock.withLock { kept.append(registration) }
    }
}

/// A chat host that keeps the text of each message the product posts.
private final class EchoChatBridge: ChatHostBridge, @unchecked Sendable {
    private let lock = NSLock()
    private var texts: [String] = []

    var posted: [String] {
        lock.withLock { texts }
    }

    func createRoom(roomId _: String, name _: String, icon _: String) async throws -> ChatRoomRegistrationStatus {
        .new
    }

    func registerBot(botId _: String, name _: String, icon _: String) async throws -> ChatBotRegistrationStatus { .new }

    func postMessage(roomId _: String, content: ChatMessageContent, alt _: String?) async throws -> String {
        if case let .text(text) = content {
            lock.withLock { texts.append(text) }
        }
        return "posted"
    }

    func setRoomFooter(roomId _: String, footer _: ChatRoomFooter) async throws {}

    func listRooms() async throws -> [ChatRoom] { [] }
}
