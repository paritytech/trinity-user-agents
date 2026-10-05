import Foundation
import AsyncExtensions
import os
import Testing
import Products
import TrUAPIHost
import UIKitExt
@testable import polkadot_app

// MARK: - Helpers

private final class FakeChatWorker: ProductChatWorking, @unchecked Sendable {
    struct Calls {
        var botStarted = 0
        var bound = 0
        var unbound = 0
        var lastUserMessage: (text: String, roomId: String?)?
        var disposed = 0
    }

    let calls = OSAllocatedUnfairLock(initialState: Calls())

    func dispose() async { calls.withLock { $0.disposed += 1 } }
    func bindMessaging(_: ProductsNativeApi.MessagingSupport) { calls.withLock { $0.bound += 1 } }
    func unbindMessaging() { calls.withLock { $0.unbound += 1 } }
    func onBotStarted() async throws { calls.withLock { $0.botStarted += 1 } }

    func onUserMessage(text: String, roomId: String?) async throws {
        calls.withLock { $0.lastUserMessage = (text, roomId) }
    }

    func renderMessage(
        messageId _: String,
        messageType _: String,
        messageData _: Data
    ) async -> AsyncThrowingStream<String, Error> {
        AsyncThrowingStream { $0.finish() }
    }

    func dispatchEvent(roomId _: String?, messageId _: String, actionId _: String, payload _: String?) async {}

    @MainActor func attach(presentationView _: ControllerBackedProtocol) {}
}

private final class FakeWorkerManager: ProductWorkerManaging, @unchecked Sendable {
    private let active = OSAllocatedUnfairLock(initialState: 0)
    private let worker: ProductChatWorking

    init(worker: ProductChatWorking) { self.worker = worker }

    var activeCount: Int { active.withLock { $0 } }

    func acquire(productId _: ProductId) async -> ProductWorkerLease {
        active.withLock { $0 += 1 }
        let token = ProductWorkerToken { [active] in active.withLock { $0 -= 1 } }
        return ProductWorkerLease(token: token, result: .success(worker))
    }
}

private func makeRustRuntime(
    execution: MockProductExecution = MockProductExecution(),
    chainConnections: MockChainConnections = MockChainConnections(),
    engine: MockJSEngine,
    renderStartupWindow: Duration = .seconds(5)
) -> ChatRustRuntime {
    ChatRustRuntime(
        productUrl: URL(string: "product://test.dot/index.js")!,
        makeExecutionModel: { _ in
            makeExecutionModel(execution: execution, chainConnections: chainConnections)
        },
        routers: ProductRoutersFacade.worker(),
        engineFactory: { engine },
        renderStartupWindow: renderStartupWindow
    )
}

private final class BotUpdateProbe: ProductBotProviding, ChatExtensionDelegate {
    let bots = AsyncStream<[ProductBot]>.makeStream()
    let changes = AsyncStream<String>.makeStream()

    func observeBots() -> AnyAsyncSequence<[ProductBot]> {
        bots.stream.eraseToAnyAsyncSequence()
    }

    func didEnableExtensions(_: Set<ChatExtension.Id>) { changes.continuation.yield("enabled") }
    func didDisableExtensions(_: Set<ChatExtension.Id>) { changes.continuation.yield("disabled") }
}

// MARK: - Tests

struct ChatRuntimeTests {
    @Test(.timeLimit(.minutes(1)))
    func reactivatedWalletReplacesBotBeforeRestartingIt() async throws {
        let owners = [NSObject(), NSObject()]
        let product = Product(id: "test.dot", name: "Test")
        let oldManager = FakeWorkerManager(worker: FakeChatWorker())
        let oldRuntime = ManagedChatRuntime(productId: product.identifier, manager: oldManager)
        try await oldRuntime.start(messagingSupport: .init(bot: nil, context: nil))
        let oldBot = ProductBot(product: product, runtime: oldRuntime, runtimeOwner: ObjectIdentifier(owners[0]))
        let replacement = ProductBot(
            product: product,
            runtime: ManagedChatRuntime(productId: product.identifier, manager: FakeWorkerManager(worker: FakeChatWorker())),
            runtimeOwner: ObjectIdentifier(owners[1])
        )
        let probe = BotUpdateProbe()
        defer {
            probe.bots.continuation.finish()
            probe.changes.continuation.finish()
        }
        let store = ChatExtensionStore(staticExtensions: [], productBotProvider: probe)
        store.delegate = probe
        store.startObserving()
        var changes = probe.changes.stream.makeAsyncIterator()
        probe.bots.continuation.yield([oldBot])
        #expect(await changes.next() == "enabled")
        #expect(oldManager.activeCount == 1)

        probe.bots.continuation.yield([replacement])

        #expect(await changes.next() == "disabled")
        #expect(await changes.next() == "enabled")
        #expect(oldManager.activeCount == 0)
        #expect(store.getChatExtensionBot(for: product.extensionId) as? ProductBot === replacement)
    }

    @Test func nativeRuntimeStartBindsMessagingAndStartsBot() async throws {
        let worker = FakeChatWorker()
        let manager = FakeWorkerManager(worker: worker)
        let runtime = ManagedChatRuntime(productId: "test.dot", manager: manager)

        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))

        #expect(worker.calls.withLock { $0.bound } == 1)
        #expect(worker.calls.withLock { $0.botStarted } == 1)
        #expect(manager.activeCount == 1)
    }

    @Test func nativeRuntimeForwardsUserMessage() async throws {
        let worker = FakeChatWorker()
        let manager = FakeWorkerManager(worker: worker)
        let runtime = ManagedChatRuntime(productId: "test.dot", manager: manager)

        try await runtime.onUserMessage(text: "hi", roomId: "r1")

        #expect(worker.calls.withLock { $0.lastUserMessage?.text } == "hi")
        #expect(worker.calls.withLock { $0.lastUserMessage?.roomId } == "r1")
    }

    @Test func nativeRuntimeDisposeUnbindsAndReleasesLock() async throws {
        let worker = FakeChatWorker()
        let manager = FakeWorkerManager(worker: worker)
        let runtime = ManagedChatRuntime(productId: "test.dot", manager: manager)

        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))
        #expect(manager.activeCount == 1)

        await runtime.dispose()

        #expect(worker.calls.withLock { $0.unbound } == 1)
        #expect(manager.activeCount == 0)
    }

    /// The execution is opened in `start`, so a runtime that never started owns
    /// nothing to close — and must not evict the live one from the core's registry.
    @Test func rustRuntimeDisposeBeforeStartTearsDownNothing() async {
        let execution = MockProductExecution()
        let chainConnections = MockChainConnections()
        let runtime = makeRustRuntime(
            execution: execution,
            chainConnections: chainConnections,
            engine: MockJSEngine()
        )

        await runtime.dispose()
        await runtime.dispose()

        #expect(execution.stopWsBridgeCallCount == 0)
        #expect(execution.closeCallCount == 0)
        #expect(chainConnections.closeAllCallCount == 0)
    }

    @Test func rustRuntimeDisposeAfterStartTearsDownExecutionOnce() async throws {
        let execution = MockProductExecution()
        let chainConnections = MockChainConnections()
        let runtime = makeRustRuntime(
            execution: execution,
            chainConnections: chainConnections,
            engine: MockJSEngine()
        )

        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))
        await runtime.dispose()
        await runtime.dispose()

        #expect(execution.stopWsBridgeCallCount == 1)
        #expect(execution.closeCallCount == 1)
        #expect(chainConnections.closeAllCallCount == 1)
    }

    /// `ProductBot` downgrades this one to a debug log, so it has to stay distinct
    /// from a real failure.
    @Test func rustRuntimeChatSeamsFailBeforeStart() async {
        let execution = MockProductExecution()
        let runtime = makeRustRuntime(execution: execution, engine: MockJSEngine())

        await #expect(throws: ChatRustRuntime.ChatSeamError.notStarted) {
            try await runtime.onUserMessage(text: "hi", roomId: "room")
        }
        #expect(execution.publishedChatActions.isEmpty)

        await runtime.dispose()
    }

    @Test func rustRuntimeChatSeamsFailAfterDispose() async throws {
        let execution = MockProductExecution()
        let runtime = makeRustRuntime(execution: execution, engine: MockJSEngine())
        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))
        await runtime.dispose()

        await #expect(throws: CancellationError.self) {
            try await runtime.onUserMessage(text: "hi", roomId: "room")
        }
        #expect(execution.publishedChatActions.isEmpty)
    }

    @Test func rustRuntimeInstallsMediaHandlerAndScriptsBeforeLoading() async throws {
        let execution = MockProductExecution()
        let engine = MockJSEngine()
        let runtime = makeRustRuntime(execution: execution, engine: engine)

        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))

        #expect(execution.startWsBridgeCallCount == 1)
        #expect(engine.mediaHandlerWasInstalledAtInitialization)
        #expect(execution.permissionRequests.isEmpty)

        #expect(engine.initializedScripts.count == 2)
        #expect(engine.initializedScripts[0]
            .content == #"window.__truapi_localhost = { url: "ws://127.0.0.1:0/?t=test" };"#)
        #expect(engine.initializedScripts[0].insertionPoint == .atDocStart)
        #expect(engine.initializedScripts[1].content.contains("freezeAndDelete"))
        #expect(engine.initializedScripts[1].insertionPoint == .atDocStart)
        #expect(!engine.evaluatedScripts.contains { $0.contains("__truapi_localhost") })
        #expect(!engine.evaluatedScripts.contains { $0.contains("freezeAndDelete") })

        await runtime.dispose()
    }

    @Test func rustRuntimeRetriesRenderUntilTheProductAttaches() async throws {
        let execution = MockProductExecution()
        execution.renderErrors = [
            ProductRuntimeError.NotConnected,
            ProductRuntimeError.NotConnected
        ]
        let runtime = makeRustRuntime(execution: execution, engine: MockJSEngine())
        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))

        let stream = await runtime.renderMessage(
            roomId: "room", messageId: "m1", messageType: "t", messageData: Data()
        )
        for try await _ in stream {}

        #expect(execution.renderRequests.count == 3)

        await runtime.dispose()
    }

    /// Anything that is not a startup race is terminal: surface it on the first
    /// attempt instead of holding the cell in a retry loop.
    @Test func rustRuntimeDoesNotRetryTerminalRenderErrors() async throws {
        let execution = MockProductExecution()
        execution.renderErrors = [ProductRuntimeError.Closed]
        let runtime = makeRustRuntime(execution: execution, engine: MockJSEngine())
        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))

        let stream = await runtime.renderMessage(
            roomId: "room", messageId: "m1", messageType: "t", messageData: Data()
        )
        await #expect(throws: ProductRuntimeError.Closed) {
            for try await _ in stream {}
        }
        #expect(execution.renderRequests.count == 1)

        await runtime.dispose()
    }

    /// A start that never happens must not leave the cell waiting forever.
    @Test func rustRuntimeFailsPendingRendersOnDispose() async throws {
        let runtime = makeRustRuntime(engine: MockJSEngine())

        let render = Task {
            await runtime.renderMessage(
                roomId: "room", messageId: "m1", messageType: "t", messageData: Data()
            )
        }
        await runtime.dispose()

        await #expect(throws: CancellationError.self) {
            for try await _ in await render.value {}
        }
    }

    @Test func rustRuntimeYieldsTypedNodesToTheConsumer() async throws {
        let execution = MockProductExecution()
        execution.renderNodes = [.string(text: "hello")]
        let runtime = makeRustRuntime(execution: execution, engine: MockJSEngine())
        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))

        var outputs: [ChatRendererOutput] = []
        for try await output in await runtime.renderMessage(
            roomId: "room", messageId: "m1", messageType: "t", messageData: Data()
        ) {
            outputs.append(output)
        }

        #expect(outputs.count == 1)
        if case let .native(node) = outputs.first, case let .string(text: text) = node {
            #expect(text == "hello")
        } else {
            Issue.record("the rust runtime must yield typed nodes, not SCALE hex")
        }

        if case let .chatMessage(roomId, messageId, messageType) =
            execution.renderRequests.first?.context {
            #expect(roomId == "room")
            #expect(messageId == "m1")
            #expect(messageType == "t")
        } else {
            Issue.record("a chat body must be rendered under a chatMessage context")
        }

        await runtime.dispose()
    }

    /// A cell can decode before `start` opens the execution. Without `.notStarted`
    /// in the transient set the render fails once and the cell is dead for the
    /// session, because `ProductMessageDecoder` never evicts.
    @Test func rustRuntimeRetriesRenderIssuedBeforeStart() async throws {
        let execution = MockProductExecution()
        execution.renderNodes = [.string(text: "late")]
        let runtime = makeRustRuntime(execution: execution, engine: MockJSEngine())

        let render = Task {
            await runtime.renderMessage(
                roomId: "room", messageId: "m1", messageType: "t", messageData: Data()
            )
        }
        // Long enough for the render to reach the retry loop with no execution.
        try await Task.sleep(for: .milliseconds(60))
        #expect(execution.renderRequests.isEmpty)

        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))

        var outputs: [ChatRendererOutput] = []
        for try await output in await render.value {
            outputs.append(output)
        }

        #expect(outputs.count == 1)
        #expect(execution.renderRequests.count == 1)

        await runtime.dispose()
    }

    /// The rust path opts out of the native bot's typing delay: its callbacks are
    /// synchronous and hold a core dispatch thread for the whole wait.
    @Test func rustChatSurfaceSendsWithoutTheTypingDelay() {
        #expect(ProductChatSurface().messageDeliveryDelay.delayDuration == 0)
    }

    /// The core rejects an empty room id coming back, so a roomless chat must fail
    /// here rather than reach the product as an unanswerable message.
    @Test func rustRuntimeRejectsRoomlessChats() async throws {
        let execution = MockProductExecution()
        let runtime = makeRustRuntime(execution: execution, engine: MockJSEngine())
        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))

        await #expect(throws: ChatRustRuntime.ChatSeamError.roomlessChat) {
            try await runtime.onUserMessage(text: "hi", roomId: nil)
        }
        #expect(execution.publishedChatActions.isEmpty)

        // The renderer context is addressed by room, so a roomless render is
        // refused before it reaches the core rather than sent with an empty id.
        let render = await runtime.renderMessage(
            roomId: nil, messageId: "m1", messageType: "t", messageData: Data()
        )
        await #expect(throws: ChatRustRuntime.ChatSeamError.roomlessChat) {
            for try await _ in render {}
        }
        #expect(execution.renderRequests.isEmpty)

        await runtime.dispose()
    }

    @Test func rustRuntimeForwardsUserMessagesAndActions() async throws {
        let execution = MockProductExecution()
        let runtime = makeRustRuntime(execution: execution, engine: MockJSEngine())
        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))

        try await runtime.onUserMessage(text: "hi", roomId: "room")
        await runtime.dispatchEvent(
            roomId: "room", messageId: "m1", messageType: "t", actionId: "a1", payload: "p"
        )

        // A user message is a chat action; a press inside a product-drawn body is
        // a renderer action. Two different surfaces.
        #expect(execution.publishedChatActions.count == 1)
        #expect(execution.publishedChatActions[0].peer == "native")
        #expect(execution.publishedChatActions[0].roomId == "room")

        if case let .messagePosted(content) = execution.publishedChatActions[0].payload,
           case let .text(text) = content {
            #expect(text == "hi")
        } else {
            Issue.record("the user message should be a posted text message")
        }

        #expect(execution.publishedRendererActions.count == 1)
        let rendererAction = try #require(execution.publishedRendererActions.first)
        #expect(rendererAction.actionId == "a1")
        #expect(rendererAction.payload == Data("p".utf8))

        if case let .chatMessage(roomId, messageId, messageType) = rendererAction.context {
            #expect(roomId == "room")
            #expect(messageId == "m1")
            #expect(messageType == "t")
        } else {
            Issue.record("the action should be addressed by a chatMessage context")
        }

        await runtime.dispose()
    }

    @Test func disposalDuringInitializationDestroysEngineBeforeProductCode() async {
        let execution = MockProductExecution()
        let engine = MockJSEngine()
        let runtime = makeRustRuntime(execution: execution, engine: engine)
        engine.onInitialize = { await runtime.dispose() }

        await #expect(throws: CancellationError.self) {
            try await runtime.start(messagingSupport: .init(bot: nil, context: nil))
        }

        #expect(engine.destroyCallCount == 1)
        #expect(engine.evaluatedScripts.isEmpty)
        #expect(execution.closeCallCount == 1)
        #expect(execution.stopWsBridgeCallCount == 1)
    }
}
