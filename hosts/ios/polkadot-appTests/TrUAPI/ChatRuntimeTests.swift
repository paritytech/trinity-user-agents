import Foundation
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

private let chatProduct: ProductId = "test.dot"

private func makeRustRuntime(
    workers: StubWorkerManager,
    renderStartupWindow: Duration = .seconds(5)
) -> TrUAPIChatHandler {
    TrUAPIChatHandler(
        productId: chatProduct,
        workers: workers,
        renderStartupWindow: renderStartupWindow
    )
}

/// Whether a start has returned, so a test can tell waiting from finishing.
private actor StartCompletion {
    private(set) var happened = false

    func mark() {
        happened = true
    }
}

/// A manager whose worker is already up, which is what chat finds whenever
/// something else, a card on screen, got there first.
private func workersRunning(_ execution: MockProductExecution) -> StubWorkerManager {
    let workers = StubWorkerManager()
    workers.publish([chatProduct: execution])
    return workers
}

// MARK: - Tests

struct ChatRuntimeTests {
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

    /// Chat is one holder of the product's worker, not its owner. A chat
    /// session that closed while a card still shows the same product must hand
    /// its reference back and leave the worker running; closing the execution
    /// here would take the card's face down with it.
    @Test func rustRuntimeReleasesTheWorkerInsteadOfClosingIt() async throws {
        let execution = MockProductExecution()
        let workers = workersRunning(execution)
        let runtime = makeRustRuntime(workers: workers)

        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))
        #expect(workers.references.acquired == [chatProduct])

        await runtime.dispose()
        await runtime.dispose()

        #expect(workers.references.released == [chatProduct])
        #expect(execution.closeCallCount == 0)
        #expect(execution.stopWsBridgeCallCount == 0)
    }

    /// A runtime that never started took no reference, so it must give none
    /// back: an unpaired release drops the count of a worker somebody else is
    /// holding, and the core stops it under them.
    @Test func rustRuntimeDisposeBeforeStartTakesAndGivesBackNothing() async {
        let workers = StubWorkerManager()
        let runtime = makeRustRuntime(workers: workers)

        await runtime.dispose()
        await runtime.dispose()

        #expect(workers.references.acquired.isEmpty)
        #expect(workers.references.released.isEmpty)
    }

    /// Every product repository emission builds fresh bots, and the store keeps
    /// the existing one and drops the duplicate. The duplicate's runtime never
    /// started, but its dispose runs, and the chat surface it would clear is the
    /// one the live bot is bound to: the product stops receiving chat while its
    /// bot looks healthy.
    @Test func rustRuntimeDisposeLeavesAnotherRuntimesBindingAlone() async throws {
        let workers = workersRunning(MockProductExecution())
        let live = makeRustRuntime(workers: workers)
        let duplicate = makeRustRuntime(workers: workers)

        try await live.start(messagingSupport: .init(bot: nil, context: nil))
        await duplicate.dispose()

        #expect(workers.context(of: chatProduct).chat.currentMessaging != nil)

        await live.dispose()
    }

    /// The bot posts its welcome message the moment `start` returns, so `start`
    /// has to wait for the worker rather than return onto an execution that is
    /// not there yet.
    @Test func rustRuntimeStartWaitsForTheWorkerToComeUp() async throws {
        let workers = StubWorkerManager()
        let execution = MockProductExecution()
        let runtime = makeRustRuntime(workers: workers)

        // Completion is observed rather than inferred from cancellation: a
        // start that returned at once is not cancelled either, so a runtime
        // that stopped waiting would pass that test.
        let returned = StartCompletion()
        let start = Task {
            try await runtime.start(messagingSupport: .init(bot: nil, context: nil))
            await returned.mark()
        }
        try await Task.sleep(for: .milliseconds(60))
        #expect(await returned.happened == false)

        workers.publish([chatProduct: execution])
        try await start.value
        #expect(await returned.happened)

        try await runtime.onUserMessage(text: "hi", roomId: "room")
        #expect(execution.publishedChatActions.count == 1)

        await runtime.dispose()
    }

    /// A chat session can close while the worker is still coming up. Giving the
    /// request back there would reach the core before the ask registered it,
    /// dropping the count of a worker another holder is drawing from.
    @Test func rustRuntimeDoesNotGiveBackARequestItHasNotTakenYet() async throws {
        let workers = StubWorkerManager(startupWindow: .milliseconds(100))
        workers.holdsTheAsk = true
        let runtime = makeRustRuntime(workers: workers)

        var asks = workers.asks.makeAsyncIterator()
        let starting = Task { try await runtime.start(messagingSupport: .init(bot: nil, context: nil)) }
        await asks.next()

        await runtime.dispose()
        workers.openTheAsk()
        _ = try? await starting.value

        #expect(workers.references.log == ["acquire \(chatProduct)", "release \(chatProduct)"])
    }

    /// A worker that never comes up must not leave the session holding a
    /// reference: the core would keep counting it and never stop the worker it
    /// eventually builds.
    @Test func rustRuntimeGivesTheReferenceBackWhenNoWorkerComes() async throws {
        let workers = StubWorkerManager(startupWindow: .milliseconds(50))
        let runtime = makeRustRuntime(workers: workers)

        await #expect(throws: (any Error).self) {
            try await runtime.start(messagingSupport: .init(bot: nil, context: nil))
        }

        #expect(workers.references.acquired == [chatProduct])
        #expect(workers.references.released == [chatProduct])
    }

    /// `ProductBot` downgrades this one to a debug log, so it has to stay distinct
    /// from a real failure.
    @Test func rustRuntimeChatSeamsFailBeforeTheWorkerIsUp() async {
        let execution = MockProductExecution()
        let runtime = makeRustRuntime(workers: StubWorkerManager())

        await #expect(throws: TrUAPIChatHandler.ChatSeamError.notStarted) {
            try await runtime.onUserMessage(text: "hi", roomId: "room")
        }
        #expect(execution.publishedChatActions.isEmpty)

        await runtime.dispose()
    }

    @Test func rustRuntimeChatSeamsFailAfterDispose() async throws {
        let execution = MockProductExecution()
        let runtime = makeRustRuntime(workers: workersRunning(execution))
        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))
        await runtime.dispose()

        await #expect(throws: CancellationError.self) {
            try await runtime.onUserMessage(text: "hi", roomId: "room")
        }
        #expect(execution.publishedChatActions.isEmpty)
    }

    @Test func rustRuntimeRetriesRenderUntilTheProductAttaches() async throws {
        let execution = MockProductExecution()
        execution.renderErrors = [
            ProductRuntimeError.NotConnected,
            ProductRuntimeError.NotConnected
        ]
        let runtime = makeRustRuntime(workers: workersRunning(execution))
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
        let runtime = makeRustRuntime(workers: workersRunning(execution))
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

    /// A worker that never comes up must not leave the cell waiting forever.
    @Test func rustRuntimeFailsPendingRendersOnDispose() async throws {
        let runtime = makeRustRuntime(workers: StubWorkerManager())

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
        let runtime = makeRustRuntime(workers: workersRunning(execution))
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

    /// A cell can decode before the product's worker is up. Without `.notStarted`
    /// in the transient set the render fails once and the cell is dead for the
    /// session, because `ProductMessageDecoder` never evicts.
    @Test func rustRuntimeRetriesRenderIssuedBeforeTheWorkerIsUp() async throws {
        let workers = StubWorkerManager()
        let execution = MockProductExecution()
        execution.renderNodes = [.string(text: "late")]
        let runtime = makeRustRuntime(workers: workers)

        let render = Task {
            await runtime.renderMessage(
                roomId: "room", messageId: "m1", messageType: "t", messageData: Data()
            )
        }
        // Long enough for the render to reach the retry loop with no worker.
        try await Task.sleep(for: .milliseconds(60))
        #expect(execution.renderRequests.isEmpty)

        workers.publish([chatProduct: execution])
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
        let runtime = makeRustRuntime(workers: workersRunning(execution))
        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))

        await #expect(throws: TrUAPIChatHandler.ChatSeamError.roomlessChat) {
            try await runtime.onUserMessage(text: "hi", roomId: nil)
        }
        #expect(execution.publishedChatActions.isEmpty)

        // The renderer context is addressed by room, so a roomless render is
        // refused before it reaches the core rather than sent with an empty id.
        let render = await runtime.renderMessage(
            roomId: nil, messageId: "m1", messageType: "t", messageData: Data()
        )
        await #expect(throws: TrUAPIChatHandler.ChatSeamError.roomlessChat) {
            for try await _ in render {}
        }
        #expect(execution.renderRequests.isEmpty)

        await runtime.dispose()
    }

    @Test func rustRuntimeForwardsUserMessagesAndActions() async throws {
        let execution = MockProductExecution()
        let runtime = makeRustRuntime(workers: workersRunning(execution))
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
}
