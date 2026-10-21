#if canImport(UIKit) && canImport(WebKit)

import Foundation
import Testing
import UIKit
import WebKit
@testable import TrUAPIHost

/// The P0.6 spikes of Chat Extension v2: the latency of a host-initiated
/// query (B5) and the cost of a 1 MiB inline resource (B4).
///
/// A web view runs a product script on the real core, and the test asks it
/// through the Swift API. The suite prints its numbers and asserts only that
/// each answer arrives. It runs only when `TRUAPI_CHAT_SPIKES` is `1`; give it
/// to `xcodebuild` as `TEST_RUNNER_TRUAPI_CHAT_SPIKES=1`.
@Suite(
    .serialized,
    .enabled(if: ProcessInfo.processInfo.environment["TRUAPI_CHAT_SPIKES"] == "1")
)
@MainActor
struct ChatSpikeTests {
    static let room = "spike"
    private static let mebibyte = 1024 * 1024

    /// B5: open a messages query, receive the first item, drop the query.
    /// "Cold" starts from no runtime. "Warm" asks a product that has run
    /// its handler before.
    @Test(.timeLimit(.minutes(5)))
    func oneItemQueryLatency() async throws {
        var cold: [Double] = []
        var coldBoot: [Double] = []
        for _ in 0 ..< 5 {
            let start = ContinuousClock.now
            let product = try await SpikeProduct.open()
            defer { product.close() }
            try await product.prepareQueries(inlineBytes: 0)
            coldBoot.append(milliseconds(since: start))
            let timing = try await product.timedMessagesQuery()
            cold.append(milliseconds(since: start))
            #expect(timing.messages == 1)
        }

        let product = try await SpikeProduct.open()
        defer { product.close() }
        try await product.prepareQueries(inlineBytes: 0)
        _ = try await product.timedMessagesQuery()

        var warm: [Double] = []
        for _ in 0 ..< 50 {
            let timing = try await product.timedMessagesQuery()
            warm.append(timing.total)
            #expect(timing.messages == 1)
        }

        report("B5 cold: runtime, execution, web view, handler (ms)", coldBoot)
        report("B5 cold: from no runtime to the first item (ms)", cold)
        report("B5 warm: open, first item and drop (ms)", warm)
    }

    /// B5 and the worker boot with the sample product of `playground/chat-v2`.
    /// `TRUAPI_CHAT_SPIKES_WORKER` names its `out/worker/index.js`. The boot
    /// ends when the worker has registered its handlers and created its rooms.
    @Test(.timeLimit(.minutes(5)))
    func sampleWorkerBootAndQuery() async throws {
        let path = try #require(ProcessInfo.processInfo.environment["TRUAPI_CHAT_SPIKES_WORKER"])
        let worker = try String(contentsOfFile: path, encoding: .utf8)
        let request = ProductChatMessagesQueryRequest(roomId: "source", anchor: .before(cursor: nil), pageSize: 40)

        var boot: [Double] = []
        var cold: [Double] = []
        for _ in 0 ..< 5 {
            let start = ContinuousClock.now
            let product = try await SpikeProduct.open(worker: worker)
            defer { product.close() }
            try await product.chat.waitForRooms(["host", "source"])
            boot.append(milliseconds(since: start))
            _ = try await product.execution.chatMessagesQuery(request)
            cold.append(milliseconds(since: start))
        }

        let product = try await SpikeProduct.open(worker: worker)
        defer { product.close() }
        try await product.chat.waitForRooms(["host", "source"])
        _ = try await product.execution.chatMessagesQuery(request)
        var warm: [Double] = []
        for _ in 0 ..< 50 {
            let start = ContinuousClock.now
            _ = try await product.execution.chatMessagesQuery(request)
            warm.append(milliseconds(since: start))
        }

        report("B5 sample worker boot: runtime to handlers and rooms (ms)", boot)
        report("B5 sample worker cold: runtime to the first messages_query item (ms)", cold)
        report("B5 sample worker warm: messages_query open, first item and drop (ms)", warm)
    }

    /// B4: a 1 MiB `ChatResource.inline` from the product to the Swift
    /// bridge, in both directions that the core uses, and the Swift half of
    /// the binding alone.
    @Test(.timeLimit(.minutes(5)))
    func inlineResourceCrossing() async throws {
        let product = try await SpikeProduct.open()
        defer { product.close() }

        for (label, bytes) in [("16 B", 16), ("1 MiB", Self.mebibyte)] {
            let feed = try await product.timedFeedUpdates(inlineBytes: bytes, count: 10)
            #expect(product.chat.receivedInlineBytes.last == bytes)
            report("B4 update_feed \(label): product call to Swift updateFeed entry (ms)", feed.arrival)
            report("B4 update_feed \(label): product call to its result (ms)", feed.roundTrip)

            try await product.prepareQueries(inlineBytes: bytes)
            var answers: [Double] = []
            for _ in 0 ..< 10 {
                let timing = try await product.timedMessagesQuery()
                #expect(timing.inlineBytes == bytes)
                answers.append(timing.total)
            }
            report("B4 messages_query page with \(label) inline: open to answer (ms)", answers)
        }

        let update = ChatFeedUpdate.new(Self.message(inlineBytes: Self.mebibyte))
        var lower: [Double] = []
        var lift: [Double] = []
        for _ in 0 ..< 20 {
            let start = ContinuousClock.now
            let buffer = FfiConverterTypeChatFeedUpdate_lower(update)
            lower.append(milliseconds(since: start))
            let liftStart = ContinuousClock.now
            let lifted = try FfiConverterTypeChatFeedUpdate_lift(buffer)
            lift.append(milliseconds(since: liftStart))
            #expect(lifted == update)
        }
        report("B4 Swift lower of a 1 MiB ChatFeedUpdate (ms)", lower)
        report("B4 Swift lift of a 1 MiB ChatFeedUpdate (ms)", lift)
    }

    static func message(inlineBytes: Int) -> ChatMessage {
        let bytes = Data((0 ..< inlineBytes).map { UInt8(truncatingIfNeeded: $0) })
        return ChatMessage(
            messageId: "m1",
            origin: ChatMessageOrigin(participantId: "product"),
            timestamp: 1,
            body: .richText(text: "image", media: [.image(
                resource: .inline(mimeType: "image/png", bytes: bytes),
                meta: ChatMediaMeta(
                    mimeType: "image/png", sizeBytes: UInt64(inlineBytes), width: 320, height: 200,
                    placeholder: nil, alt: "spike"
                )
            )]),
            status: .sent,
            replyTo: nil,
            editedAt: nil,
            reactions: []
        )
    }

    private func milliseconds(since start: ContinuousClock.Instant) -> Double {
        let elapsed = ContinuousClock.now - start
        return Double(elapsed.components.seconds) * 1000
            + Double(elapsed.components.attoseconds) / 1e15
    }

    private func report(_ label: String, _ values: [Double]) {
        let sorted = values.sorted()
        let median = sorted[sorted.count / 2]
        let p90 = sorted[min(sorted.count - 1, Int(Double(sorted.count) * 0.9))]
        let line = String(
            format: "[spike] %@: n=%d min=%.2f median=%.2f p90=%.2f max=%.2f",
            label, sorted.count, sorted.first!, median, p90, sorted.last!
        )
        print(line)
    }
}

/// The time of one messages query, in milliseconds: open, first item and
/// drop.
private struct QueryTiming {
    let total: Double
    let messages: Int
    let inlineBytes: Int
}

/// The times of a list of feed updates, in milliseconds.
private struct FeedTiming {
    var arrival: [Double] = []
    var roundTrip: [Double] = []
}

/// One product page on the real core, with a chat bridge that records what
/// it receives.
@MainActor
private struct SpikeProduct {
    let execution: TrUAPIProductExecution
    let chat: SpikeChatBridge
    let webView: WKWebView
    let window: UIWindow

    static func open(worker: String? = nil) async throws -> SpikeProduct {
        let bridge = StubHostBridge()
        let chat = SpikeChatBridge()
        var config = try TrUAPIWsBridgeTests.makeHostRuntimeConfig()
        config.localSessionSecret = Data(repeating: 0xAB, count: 32)
        let runtime = try TrUAPIHostRuntime(bridge: bridge, runtimeConfig: config)
        let execution = try runtime.openProductExecution(
            bridge: bridge,
            configuration: ProductExecutionConfig(productId: "spike.dot", executionKind: .worker),
            chat: chat
        )
        let configuration = WKWebViewConfiguration()
        configuration.preferences.isFraudulentWebsiteWarningEnabled = false
        let webView = WKWebView(frame: .zero, configuration: configuration)
        let scene = try #require(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UIViewController()
        window.rootViewController?.view.addSubview(webView)
        window.isHidden = false
        try TrUAPIHost.installProductScripts(into: webView, endpoint: execution.startWsBridge(bindPort: 0))
        let product = SpikeProduct(execution: execution, chat: chat, webView: webView, window: window)
        let script = worker.map {
            "<script type=\"module\">\($0.replacingOccurrences(of: "</script", with: "<\\/script"))</script>"
        } ?? ""
        webView.loadHTMLString("<html><body>spike\(script)</body></html>", baseURL: URL(string: "http://localhost/"))
        try await product.waitForClient()
        return product
    }

    func close() {
        webView.stopLoading()
        window.isHidden = true
        window.rootViewController = nil
        execution.stopWsBridge()
        execution.close()
    }

    func waitForClient() async throws {
        let deadline = ContinuousClock.now + .seconds(30)
        while ContinuousClock.now < deadline {
            let ready = try? await webView.callAsyncJavaScript(
                "return typeof window.__HOST_API_CLIENT__ === 'object' && !!window.__HOST_API_CLIENT__.client",
                arguments: [:], in: nil, contentWorld: .page
            ) as? Bool
            if ready == true { return }
            try await Task.sleep(for: .milliseconds(5))
        }
        throw SpikeError.failed("the product client did not appear")
    }

    /// Creates the product-source room, and makes the handler answer one
    /// message with an inline image of `inlineBytes`, or a text without one.
    func prepareQueries(inlineBytes: Int) async throws {
        try await ensureImageBuilder()
        let result = try await webView.callAsyncJavaScript("""
            const client = window.__HOST_API_CLIENT__.client;
            const room = await client.chat.createRoom({
              roomId: room_id,
              metadata: { name: 'Spike' },
              config: {
                productSource: true, reactionDetails: false, participants: false,
                messageActions: false, reportSeen: false,
              },
            });
            if (room.isErr()) return 'create_room ' + JSON.stringify(room.error);
            const body = inline_bytes === 0
              ? { tag: 'Text', value: { text: 'hello' } }
              : window.__spikeImage(inline_bytes);
            window.__spikeHandler?.unsubscribe();
            window.__spikeHandler = client.chat.onMessagesQuery((request, send) => {
              send({
                messages: [{
                  messageId: 'm1', origin: { participantId: 'product' }, timestamp: 1n,
                  body, status: 'Sent', reactions: [],
                }],
              });
            });
            return 'ok';
            """, arguments: ["room_id": ChatSpikeTests.room, "inline_bytes": inlineBytes],
            in: nil, contentWorld: .page)
        guard result as? String == "ok" else { throw SpikeError.failed("prepare: \(String(describing: result))") }
    }

    /// Adds `window.__spikeImage(n)`, a rich text body with one inline
    /// image of `n` bytes.
    func ensureImageBuilder() async throws {
        _ = try await webView.callAsyncJavaScript("""
            window.__spikeImage = (n) => {
              const hex = new Array(n);
              for (let i = 0; i < n; i++) hex[i] = (i & 0xff).toString(16).padStart(2, '0');
              return {
                tag: 'RichText',
                value: {
                  text: 'image',
                  media: [{ tag: 'Image', value: {
                    resource: { tag: 'Inline', value: { mimeType: 'image/png', bytes: '0x' + hex.join('') } },
                    meta: { mimeType: 'image/png', sizeBytes: BigInt(n), width: 320, height: 200, alt: 'spike' },
                  } }],
                },
              };
            };
            return true;
            """, arguments: [:], in: nil, contentWorld: .page)
    }

    /// Opens one messages query through `TrUAPIProductExecution`, which
    /// opens the native query, waits for the first item and drops the query.
    func timedMessagesQuery() async throws -> QueryTiming {
        let request = ProductChatMessagesQueryRequest(
            roomId: ChatSpikeTests.room, anchor: .before(cursor: nil), pageSize: 40
        )
        let start = ContinuousClock.now
        let page = try await execution.chatMessagesQuery(request)
        let total = ms(ContinuousClock.now - start)

        var inlineBytes = 0
        if case let .richText(_, media)? = page.messages.first?.body,
           case let .image(.inline(_, bytes), _)? = media.first {
            inlineBytes = bytes.count
        }
        return QueryTiming(total: total, messages: page.messages.count, inlineBytes: inlineBytes)
    }

    /// Sends `count` feed updates of one message with an inline image of
    /// `inlineBytes`, one after the other and under the rate limit.
    func timedFeedUpdates(inlineBytes: Int, count: Int) async throws -> FeedTiming {
        try await ensureImageBuilder()
        _ = try await webView.callAsyncJavaScript("""
            const client = window.__HOST_API_CLIENT__.client;
            const room = await client.chat.createRoom({
              roomId: 'feed', metadata: { name: 'Feed' },
              config: {
                productSource: false, reactionDetails: false, participants: false,
                messageActions: false, reportSeen: false,
              },
            });
            return room.isOk();
            """, arguments: [:], in: nil, contentWorld: .page)

        var timing = FeedTiming()
        for index in 0 ..< count {
            let result = try await webView.callAsyncJavaScript("""
                const client = window.__HOST_API_CLIENT__.client;
                const message = {
                  messageId: 'f' + index, origin: { participantId: 'product' }, timestamp: BigInt(index + 1),
                  body: window.__spikeImage(inline_bytes), status: 'Sent', reactions: [],
                };
                const started = performance.timeOrigin + performance.now();
                const result = await client.chat.updateFeed({ roomId: 'feed', update: { tag: 'New', value: message } });
                const ended = performance.timeOrigin + performance.now();
                return JSON.stringify({ ok: result.isOk(), error: result.isErr() ? String(JSON.stringify(result.error)) : null, started, ended });
                """, arguments: ["index": index, "inline_bytes": max(inlineBytes, 1)],
                in: nil, contentWorld: .page) as? String
            let decoded = try JSONDecoder().decode(FeedCall.self, from: Data((result ?? "{}").utf8))
            guard decoded.ok else { throw SpikeError.failed("update_feed: \(decoded.error ?? "?")") }
            guard let arrived = chat.lastFeedArrival else { throw SpikeError.failed("no updateFeed call") }
            timing.arrival.append(arrived * 1000 - decoded.started)
            timing.roundTrip.append(decoded.ended - decoded.started)
            try await Task.sleep(for: .milliseconds(100))
        }
        return timing
    }

    private func ms(_ duration: Duration) -> Double {
        Double(duration.components.seconds) * 1000 + Double(duration.components.attoseconds) / 1e15
    }
}

private struct FeedCall: Decodable {
    let ok: Bool
    let error: String?
    let started: Double
    let ended: Double
}

private enum SpikeError: Error {
    case failed(String)
}

/// A chat bridge that keeps its rooms and configurations, and records when
/// each feed update arrives and the size of its inline image.
private final class SpikeChatBridge: ChatHostBridge, @unchecked Sendable {
    private let lock = NSLock()
    private var rooms: [String: ChatRoomConfig] = [:]
    private var arrivals: [Double] = []
    private var inlineSizes: [Int] = []

    var lastFeedArrival: Double? { lock.withLock { arrivals.last } }

    /// Waits until the product has created each room of `roomIds`.
    func waitForRooms(_ roomIds: Set<String>) async throws {
        let deadline = ContinuousClock.now + .seconds(30)
        while ContinuousClock.now < deadline {
            if lock.withLock({ roomIds.isSubset(of: rooms.keys) }) { return }
            try await Task.sleep(for: .milliseconds(2))
        }
        throw SpikeError.failed("the product did not create \(roomIds)")
    }
    var receivedInlineBytes: [Int] { lock.withLock { inlineSizes } }

    func createRoom(roomId: String, name _: String, icon _: String) async throws -> ChatRoomRegistrationStatus {
        lock.withLock {
            if rooms[roomId] != nil { return .exists }
            rooms[roomId] = .hostDefault
            return .new
        }
    }

    func registerBot(botId _: String, name _: String, icon _: String) async throws -> ChatBotRegistrationStatus { .new }

    func postMessage(roomId _: String, content _: ChatMessageContent, alt _: String?) async throws -> String { "posted" }

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

    func updateFeed(roomId _: String, update: ChatFeedUpdate, persist _: Bool) async throws {
        let arrived = Date().timeIntervalSince1970
        var size = 0
        if case let .new(message) = update,
           case let .richText(_, media) = message.body,
           case let .image(.inline(_, bytes), _)? = media.first {
            size = bytes.count
        }
        lock.withLock {
            arrivals.append(arrived)
            inlineSizes.append(size)
        }
    }

}

#endif
