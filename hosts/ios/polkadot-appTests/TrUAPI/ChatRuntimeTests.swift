import Foundation
import Keystore_iOS
import os
import PolkadotUI
import Products
import Testing
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

private let chatProduct: ProductId = try! ProductId("test.dot")

private func makeRustRuntime(
    workers: StubWorkerRuntime,
    context: ProductWorkerContext = ProductWorkerContext()
) -> TrUAPIChatHandler {
    TrUAPIChatHandler(productId: chatProduct, runtime: { workers }, context: context)
}

/// Runs `body` and disposes `runtime` however it ends. A started handler
/// dropped without `dispose()` trips its deinit assertion, which takes the
/// whole test process down and hides the failure that threw.
private func disposing(_ runtime: TrUAPIChatHandler, _ body: () async throws -> Void) async throws {
    do {
        try await body()
    } catch {
        await runtime.dispose()
        throw error
    }
    await runtime.dispose()
}

private extension TrUAPIChatHandler {
    /// Return after the send has entered Chat's actor and yielded it to the test.
    func beginSending(_ text: String) async -> Task<Void, Error> {
        let entered = AsyncStream<Void>.makeStream()
        let send = Task {
            entered.continuation.finish()
            try await onUserMessage(text: text, roomId: "room")
        }
        for await _ in entered.stream {}
        return send
    }
}

/// A manager whose worker is already up, which is what chat finds whenever
/// something else, a card on screen, got there first.
private func workersRunning(_ execution: MockProductExecution) -> StubWorkerRuntime {
    let workers = StubWorkerRuntime()
    workers.publish([chatProduct: execution])
    return workers
}

private func makeFooterDecoder(runtime: TrUAPIChatHandler) -> ProductMessageDecoder {
    ProductMessageDecoder(
        runtime: runtime,
        tokenResolver: StubWidgetDesignTokenResolver(),
        description: nil,
        resolveImage: WidgetImageResolver { _ in nil },
        logger: Logger.shared
    )
}

private func dockedCard(
    from decoder: ProductMessageDecoder,
    payload: Data = Data([1])
) throws -> ProductWidgetViewModel {
    let card = try #require(
        decoder.makeRoomFooterCard(roomId: "room", messageType: "register", payload: payload)
            as? ProductRoomFooterCardConfiguration
    )
    return try #require(card.nodeProvider as? ProductWidgetViewModel)
}

// MARK: - Tests

struct ChatRuntimeTests {
    @Test func nativeRuntimeStartBindsMessagingAndStartsBot() async throws {
        let worker = FakeChatWorker()
        let manager = FakeWorkerManager(worker: worker)
        let runtime = ManagedChatRuntime(productId: try! ProductId("test.dot"), manager: manager)

        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))

        #expect(worker.calls.withLock { $0.bound } == 1)
        #expect(worker.calls.withLock { $0.botStarted } == 1)
        #expect(manager.activeCount == 1)
    }

    @Test func nativeRuntimeForwardsUserMessage() async throws {
        let worker = FakeChatWorker()
        let manager = FakeWorkerManager(worker: worker)
        let runtime = ManagedChatRuntime(productId: try! ProductId("test.dot"), manager: manager)

        try await runtime.onUserMessage(text: "hi", roomId: "r1")

        #expect(worker.calls.withLock { $0.lastUserMessage?.text } == "hi")
        #expect(worker.calls.withLock { $0.lastUserMessage?.roomId } == "r1")
    }

    @Test func nativeRuntimeDisposeUnbindsAndReleasesLock() async throws {
        let worker = FakeChatWorker()
        let manager = FakeWorkerManager(worker: worker)
        let runtime = ManagedChatRuntime(productId: try! ProductId("test.dot"), manager: manager)

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

    /// Every product repository emission builds fresh bots, and the store keeps
    /// the existing one and drops the duplicate. The duplicate's runtime never
    /// started, but its dispose runs, and the chat surface it would clear is the
    /// one the live bot is bound to: the product stops receiving chat while its
    /// bot looks healthy.
    @Test func rustRuntimeDisposeLeavesAnotherRuntimesBindingAlone() async throws {
        let workers = workersRunning(MockProductExecution())
        let contexts = ProductWorkerContexts(presentationFallback: { nil })
        let live = makeRustRuntime(workers: workers, context: contexts.context(for: chatProduct))
        let duplicate = makeRustRuntime(workers: workers, context: contexts.context(for: chatProduct))

        try await live.start(messagingSupport: .init(bot: nil, context: nil))
        await duplicate.dispose()
        await duplicate.dispose()

        #expect(workers.references.releaseCounts == [0])
        #expect(contexts.context(for: chatProduct).chat.currentMessaging != nil)

        await live.dispose()
    }

    @Test(.timeLimit(.minutes(1)))
    func chatStartsBeforeTheWorkerAndDeliversPendingMessagesInOrder() async throws {
        let workers = StubWorkerRuntime()
        let execution = MockProductExecution()
        let runtime = makeRustRuntime(workers: workers)
        var states = workers.executionsDelivered.makeAsyncIterator()

        try await disposing(runtime) {
            try await runtime.start(messagingSupport: .init(bot: nil, context: nil))
            _ = await states.next()

            let first = await runtime.beginSending("first")
            _ = await states.next()
            let second = await runtime.beginSending("second")
            workers.publish([chatProduct: execution])

            try await first.value
            try await second.value
            #expect(execution.publishedChatActions == ["first", "second"].map {
                HostChatActionSubscribeItem(roomId: "room", peer: "native", payload: .messagePosted(.text(text: $0)))
            })
        }
    }

    @Test(.timeLimit(.minutes(1)))
    func closingChatCancelsAPendingSendWhilePocketStillHoldsTheWorker() async throws {
        let workers = StubWorkerRuntime()
        let pocket = workers.claimWorker(productId: chatProduct)
        defer { pocket.release() }
        let execution = MockProductExecution()
        let runtime = makeRustRuntime(workers: workers)
        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))
        _ = await workers.executionsDelivered.first { _ in true }
        await withTaskGroup(of: Void.self) { group in
            group.addTask {
                _ = await workers.executionsDelivered.first { _ in true }
                await runtime.dispose()
            }
            await #expect(throws: CancellationError.self) {
                try await runtime.onUserMessage(text: "pending", roomId: "room")
            }
        }

        #expect(workers.references.releaseCounts == [0, 1])
        workers.publish([chatProduct: execution])
        await #expect(throws: CancellationError.self) {
            try await runtime.onUserMessage(text: "after close", roomId: "room")
        }
        #expect(execution.publishedChatActions.isEmpty)
    }

    /// `ProductBot` downgrades this one to a debug log, so it has to stay distinct
    /// from a real failure.
    @Test func chatCannotSendBeforeStartingEvenWhenTheWorkerIsRunning() async {
        let execution = MockProductExecution()
        let runtime = makeRustRuntime(workers: workersRunning(execution))

        await #expect(throws: TrUAPIChatHandler.ChatSeamError.notStarted) {
            try await runtime.onUserMessage(text: "hi", roomId: "room")
        }
        #expect(execution.publishedChatActions.isEmpty)

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
    @Test(.timeLimit(.minutes(1)))
    func rustRuntimeFailsPendingRendersOnDispose() async throws {
        let workers = StubWorkerRuntime()
        let runtime = makeRustRuntime(workers: workers)
        let render = await runtime.renderMessage(
            roomId: "room", messageId: "m1", messageType: "t", messageData: Data()
        )
        _ = await workers.executionsDelivered.first { _ in true }
        await runtime.dispose()

        await #expect(throws: CancellationError.self) {
            for try await _ in render {}
        }
    }

    /// A cell can decode before the product's worker is up. The render waits for
    /// the worker: failing once would leave the cell dead for the session,
    /// because `ProductMessageDecoder` never evicts.
    @Test(.timeLimit(.minutes(1)))
    func rustRuntimeRenderWaitsForTheWorkerToComeUp() async throws {
        let workers = StubWorkerRuntime()
        let execution = MockProductExecution()
        execution.renderNodes = [.string(text: "late")]
        let runtime = makeRustRuntime(workers: workers)

        try await disposing(runtime) {
            let render = await runtime.renderMessage(
                roomId: "room", messageId: "m1", messageType: "t", messageData: Data([1])
            )
            _ = await workers.executionsDelivered.first { _ in true }
            workers.publish([chatProduct: execution])
            try await runtime.start(messagingSupport: .init(bot: nil, context: nil))

            var texts: [String] = []
            for try await output in render {
                guard case let .native(.string(text)) = output else {
                    Issue.record("Expected a native text node")
                    continue
                }
                texts.append(text)
            }

            #expect(texts == ["late"])
            #expect(execution.renderRequests == [ProductRendererRenderRequest(
                context: .chatMessage(roomId: "room", messageId: "m1", messageType: "t"),
                payload: Data([1])
            )])
        }
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

    /// A worker release restarts the worker under the open chat; the handler must reach the new
    /// worker without asking for it again, since the core still counts the request it holds.
    @Test func rustRuntimeFollowsARestartedWorker() async throws {
        let previous = MockProductExecution()
        let workers = workersRunning(previous)
        let runtime = makeRustRuntime(workers: workers)
        try await disposing(runtime) {
            try await runtime.start(messagingSupport: .init(bot: nil, context: nil))

            try await runtime.onUserMessage(text: "before", roomId: "room")
            let restarted = MockProductExecution()
            workers.publish([chatProduct: restarted])
            try await runtime.onUserMessage(text: "after", roomId: "room")
            await runtime.dispatchEvent(
                roomId: "room", messageId: "m1", messageType: "t", actionId: "join", payload: "p"
            )

            #expect(previous.publishedChatActions == [HostChatActionSubscribeItem(
                roomId: "room", peer: "native", payload: .messagePosted(.text(text: "before"))
            )])
            #expect(previous.publishedRendererActions.isEmpty)
            #expect(restarted.publishedChatActions == [HostChatActionSubscribeItem(
                roomId: "room", peer: "native", payload: .messagePosted(.text(text: "after"))
            )])
            #expect(restarted.publishedRendererActions == [HostRendererActionSubscribeItem(
                context: .chatMessage(roomId: "room", messageId: "m1", messageType: "t"),
                actionId: "join", payload: Data("p".utf8)
            )])
            #expect(workers.references.acquired == [chatProduct])
            #expect(workers.references.released.isEmpty)
        }
    }

    @Test(.timeLimit(.minutes(1)))
    func chatForwardsTheCurrentRoomsAndUpdatesToEachRunningWorker() async throws {
        let bot = ChatSampleExtension(logger: StubLogger())
        let rooms = ChatExtensionDiscoverContext(
            settings: InMemorySettingsManager(),
            storageFacade: UserDataStorageTestFacade(),
            operationQueue: OperationQueue(),
            logger: StubLogger()
        )
        let first = MockProductExecution()
        let replacement = MockProductExecution()
        let workers = workersRunning(first)
        let runtime = makeRustRuntime(workers: workers)
        var firstUpdates = first.roomChanges.stream.makeAsyncIterator()
        var replacementUpdates = replacement.roomChanges.stream.makeAsyncIterator()
        var states = workers.executionsDelivered.makeAsyncIterator()
        let lobby = ChatRoom(roomId: "lobby", participatingAs: .roomHost)
        let news = ChatRoom(roomId: "news", participatingAs: .roomHost)

        try await disposing(runtime) {
            try await runtime.start(messagingSupport: .init(bot: bot, context: rooms))
            _ = await states.next()
            #expect(await firstUpdates.next() == [])

            _ = try await rooms.createRoom(for: bot, roomId: "lobby", name: nil, icon: nil)
            #expect(await firstUpdates.next() == [lobby])

            workers.publish([:])
            _ = await states.next()
            workers.publish([chatProduct: replacement])
            #expect(await replacementUpdates.next() == [lobby])

            _ = try await rooms.createRoom(for: bot, roomId: "news", name: nil, icon: nil)
            #expect(await replacementUpdates.next()?.sorted { $0.roomId < $1.roomId } == [lobby, news])
        }

        first.roomChanges.continuation.finish()
        #expect(await firstUpdates.next() == nil)
        #expect(workers.readinessObservations == (started: 1, cancelled: 1))
        #expect(workers.references.released == [chatProduct])
    }

    @Test func rustRuntimeRendersADockedFooterCardUnderTheFooterContext() async throws {
        let execution = MockProductExecution()
        execution.renderNodes = [.string(text: "docked")]
        let runtime = makeRustRuntime(workers: workersRunning(execution))
        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))

        for try await _ in await runtime.renderRoomFooter(
            roomId: "room", messageType: "register", payload: Data([1])
        ) {}

        let request = try #require(execution.renderRequests.first)
        #expect(request.payload == Data([1]))
        if case let .chatRoomFooter(roomId, messageType) = request.context {
            #expect(roomId == "room")
            #expect(messageType == "register")
        } else {
            Issue.record("a footer card must be rendered under a chatRoomFooter context")
        }

        await runtime.dispose()
    }

    /// The core routes a press by context, so a footer press addressed as a chat
    /// message would reach no card the product drew.
    @Test func rustRuntimePublishesAFooterTapUnderTheFooterContext() async throws {
        let execution = MockProductExecution()
        let runtime = makeRustRuntime(workers: workersRunning(execution))
        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))

        await runtime.dispatchRoomFooterEvent(
            roomId: "room", messageType: "register", actionId: "join", payload: "p"
        )

        let rendererAction = try #require(execution.publishedRendererActions.first)
        #expect(execution.publishedRendererActions.count == 1)
        #expect(rendererAction.actionId == "join")
        #expect(rendererAction.payload == Data("p".utf8))
        if case let .chatRoomFooter(roomId, messageType) = rendererAction.context {
            #expect(roomId == "room")
            #expect(messageType == "register")
        } else {
            Issue.record("a footer press must be addressed by a chatRoomFooter context")
        }

        await runtime.dispose()
    }

    /// A worker still booting when the room opens ends the first footer stream
    /// before it draws, and the card has to appear once the product can draw it.
    @Test func aDockedCardWhoseFirstStreamDrewNothingIsReopened() async throws {
        let execution = MockProductExecution()
        let runtime = makeRustRuntime(workers: workersRunning(execution))
        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))
        let viewModel = try dockedCard(from: makeFooterDecoder(runtime: runtime))

        await execution.awaitRenderRequest()
        execution.renderNodes = [.spacer(modifiers: [])]
        await viewModel.renderTask?.value

        #expect(execution.renderRequests.count == 2)
        let node = await MainActor.run { viewModel.node }
        #expect(node != nil)

        await runtime.dispose()
    }

    /// The chat rebuilds its input configuration on every feed update, and each
    /// fresh card would open another render stream on the product.
    @Test func aFeedUpdateKeepsTheDockedCardItAlreadyRenders() async throws {
        let execution = MockProductExecution()
        execution.keepsRenderStreamOpen = true
        let runtime = makeRustRuntime(workers: workersRunning(execution))
        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))
        let decoder = makeFooterDecoder(runtime: runtime)

        let first = try dockedCard(from: decoder)
        let second = try dockedCard(from: decoder)

        #expect(first === second)

        await runtime.dispose()
    }

    /// The native runtime cannot draw a docked card, and a room that shows
    /// neither its input nor its card must say why in the host log, once per
    /// card rather than on every feed update.
    @Test func aCardDockedUnderTheNativeRuntimeIsLoggedAsRefused() throws {
        let runtime = try ManagedChatRuntime(
            productId: ProductId("test.dot"),
            manager: FakeWorkerManager(worker: FakeChatWorker())
        )
        let logger = RecordingLogger()
        let decoder = ProductMessageDecoder(
            runtime: runtime,
            tokenResolver: StubWidgetDesignTokenResolver(),
            description: nil,
            resolveImage: WidgetImageResolver { _ in nil },
            logger: logger
        )

        let card = decoder.makeRoomFooterCard(roomId: "room", messageType: "register", payload: Data([1]))
        let sameCard = decoder.makeRoomFooterCard(roomId: "room", messageType: "register", payload: Data([1]))
        let nextCard = decoder.makeRoomFooterCard(roomId: "room", messageType: "register", payload: Data([2]))

        #expect(card == nil)
        #expect(sameCard == nil)
        #expect(nextCard == nil)
        #expect(logger.lines == [
            .warning("[truapi.chat] A card is docked, but only the TrUAPI chat runtime can draw one"),
            .warning("[truapi.chat] A card is docked, but only the TrUAPI chat runtime can draw one"),
        ])
    }

    /// The product replaces its card on every update and may clear it, and each
    /// card left drawing would hold a render stream open on the product.
    @Test func aReplacedOrClearedCardStopsDrawing() async throws {
        let execution = MockProductExecution()
        execution.keepsRenderStreamOpen = true
        let runtime = makeRustRuntime(workers: workersRunning(execution))
        try await runtime.start(messagingSupport: .init(bot: nil, context: nil))
        let decoder = makeFooterDecoder(runtime: runtime)

        // Given
        let first = try dockedCard(from: decoder, payload: Data([1]))
        await execution.awaitRenderRequest()

        // When
        let second = try dockedCard(from: decoder, payload: Data([2]))
        await execution.awaitRenderRequest()

        // Then
        try #require(first.renderTask?.isCancelled == true)
        await first.renderTask?.value
        #expect(second.renderTask?.isCancelled == false)

        // When
        decoder.clearRoomFooterCard(roomId: "room")

        // Then
        try #require(second.renderTask?.isCancelled == true)
        await second.renderTask?.value
        #expect(execution.renderRequests.count == 2)
        #expect(try dockedCard(from: decoder, payload: Data([2])) !== second)

        await runtime.dispose()
    }
}
