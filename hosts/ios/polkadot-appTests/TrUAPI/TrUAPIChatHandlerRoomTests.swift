import Foundation
import PolkadotUI
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The room screen holds the worker of its product while it shows a room, and each query holds it while it runs
/// (P5.4, 02-DESIGN §8.4 and D10; scenario S24).
struct TrUAPIChatHandlerRoomTests {
    private let product: ProductId = "test.dot"
    private let execution = MockProductExecution()
    private let activity = ChatRoomActivityStore()

    @Test func anOpenHoldsTheWorkerAndSendsRoomOpened() async throws {
        let workers = runningWorkers()
        let handler = makeHandler(workers)

        try await handler.open(roomId: "source")

        #expect(workers.references.acquired == [product])
        #expect(workers.references.released.isEmpty)
        #expect(execution.publishedChatEvents == [.init(roomId: "source", peer: "native", event: .roomOpened)])

        await handler.dispose()
    }

    @Test func aCloseSendsRoomClosedClearsTheActivityAndReleasesTheWorker() async throws {
        let workers = runningWorkers()
        let handler = makeHandler(workers)
        let chatId = Chat.Id.chatExtension(product, roomId: "source")
        activity.set(.init(text: "Sample is typing", participantId: nil), for: chatId)

        try await handler.open(roomId: "source")
        await handler.close(roomId: "source")

        #expect(execution.publishedChatEvents.map(\.event) == [.roomOpened, .roomClosed])
        #expect(activity.activity(for: chatId) == nil)
        #expect(workers.references.log == ["acquire \(product)", "release \(product)"])

        await handler.dispose()
        #expect(workers.references.released == [product])
    }

    @Test func aCloseWithoutAnOpenDoesNothing() async {
        let workers = runningWorkers()
        let handler = makeHandler(workers)

        await handler.close(roomId: "source")

        #expect(execution.publishedChatEvents.isEmpty)
        #expect(workers.references.released.isEmpty)

        await handler.dispose()
    }

    /// Two screens of one room each hold the worker, so the worker stops only after the second close.
    @Test func eachOpenNeedsItsOwnClose() async throws {
        let workers = runningWorkers()
        let handler = makeHandler(workers)

        try await handler.open(roomId: "source")
        try await handler.open(roomId: "source")
        await handler.close(roomId: "source")
        #expect(workers.references.released.count == 1)

        await handler.close(roomId: "source")
        await handler.close(roomId: "source")
        #expect(workers.references.released.count == 2)

        await handler.dispose()
        #expect(workers.references.released.count == 2)
    }

    @Test func aWorkerThatDoesNotStartFailsTheOpenAndHoldsNothing() async {
        let workers = StubWorkerManager(startupWindow: .milliseconds(50))
        let handler = makeHandler(workers)

        await #expect(throws: (any Error).self) {
            try await handler.open(roomId: "source")
        }
        await handler.close(roomId: "source")

        #expect(workers.references.log == ["acquire \(product)", "release \(product)"])
        #expect(execution.publishedChatEvents.isEmpty)

        await handler.dispose()
    }

    @Test func aDisposeReleasesTheOpenRooms() async throws {
        let workers = runningWorkers()
        let handler = makeHandler(workers)

        try await handler.open(roomId: "host")
        try await handler.open(roomId: "source")
        await handler.dispose()

        #expect(workers.references.released == [product, product])
        await #expect(throws: CancellationError.self) {
            try await handler.open(roomId: "host")
        }
    }

    @Test func aPageQueryHoldsTheWorkerOnlyWhileItRuns() async throws {
        let workers = runningWorkers()
        let handler = makeHandler(workers)
        let page = ProductChatMessagesQueryItem(messages: [], next: nil)
        execution.messagesAnswer = .success(page)

        let answer = try await handler.messagesPage(ProductChatMessagesQueryRequest(
            roomId: "source",
            anchor: .before(cursor: nil),
            pageSize: 40
        ))

        #expect(answer == page)
        #expect(workers.references.log == ["acquire \(product)", "release \(product)"])
        guard case let .messages(_, timeout) = execution.queryLog.first else {
            Issue.record("the page query must reach the product")
            return
        }
        #expect(timeout == ChatQueryTimeout.messages)

        await handler.dispose()
    }

    @Test func aUserActionUsesTheTimeoutOfItsKindAndPassesTheRefusalOn() async throws {
        let workers = runningWorkers()
        let handler = makeHandler(workers)
        execution.userActionAnswer = .failure(ChatQueryError.actionRefused(.rejected(reason: "no")))
        let action = ChatUserAction.newMessage(body: .text(text: "hi"), replyTo: nil)

        await #expect(throws: ChatQueryError.actionRefused(.rejected(reason: "no"))) {
            try await handler.userAction(ProductChatUserActionRequest(roomId: "source", action: action))
        }

        guard case let .userAction(_, timeout) = execution.queryLog.first else {
            Issue.record("the user action must reach the product")
            return
        }
        #expect(timeout == ChatQueryTimeout.timeout(for: action))
        #expect(workers.references.released == [product])

        await handler.dispose()
    }

    /// The room screen finds the lifecycle and the queries through the bot of the product.
    @Test func theBotOfAProductServesTheRoomsOfItsRuntime() async throws {
        let workers = runningWorkers()
        let handler = makeHandler(workers)
        let bot = ProductBot(
            product: Product(id: product, name: "Test"),
            description: nil,
            runtime: handler,
            resolveImage: WidgetImageResolver { _ in nil }
        )

        let services = ProductRoomServices(bot: bot)
        try await services.lifecycle.open(roomId: "source")
        await services.lifecycle.close(roomId: "source")

        #expect(services.querying != nil)
        #expect(execution.publishedChatEvents.map(\.event) == [.roomOpened, .roomClosed])

        await bot.dispose()
    }
}

extension TrUAPIChatHandlerRoomTests {
    /// The open room holds the worker, so the worker stops under it only when it fails (06-OPEN-ITEMS O26). The
    /// room then shows "unavailable", and the next worker hears `RoomOpened` again (S23).
    @Test func aWorkerThatStopsUnderAnOpenRoomMakesItUnavailableAndTheNextWorkerHearsRoomOpened() async throws {
        let workers = runningWorkers()
        let listeners = ProductFeedListeners()
        let listener = StopRecordingListener()
        let registration = listeners.register(listener, for: .chatExtension(product, roomId: "source"))
        let handler = TrUAPIChatHandler(
            productId: product,
            workers: workers,
            activityStore: activity,
            firstRoomHold: nil,
            feedListeners: listeners
        )

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        try await Task.sleep(for: .milliseconds(50))
        try await handler.open(roomId: "source")

        workers.publish([:])
        try await poll { await listener.stops == 1 }

        let next = MockProductExecution()
        workers.publish([product: next])
        try await poll { !next.publishedChatEvents.isEmpty }

        #expect(next.publishedChatEvents == [.init(roomId: "source", peer: "native", event: .roomOpened)])

        await handler.dispose()
        registration.cancel()
    }

    @Test func aWorkerThatStopsWithNoOpenRoomTellsNoRoom() async throws {
        let workers = runningWorkers()
        let listeners = ProductFeedListeners()
        let listener = StopRecordingListener()
        let registration = listeners.register(listener, for: .chatExtension(product, roomId: "source"))
        let handler = TrUAPIChatHandler(
            productId: product,
            workers: workers,
            activityStore: activity,
            firstRoomHold: nil,
            feedListeners: listeners
        )

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        try await Task.sleep(for: .milliseconds(50))
        workers.publish([:])
        try await Task.sleep(for: .milliseconds(100))

        #expect(await listener.stops == 0)

        await handler.dispose()
        registration.cancel()
    }
}

private actor StopRecordingListener: ProductFeedListening {
    private(set) var stops = 0

    func apply(roomId _: String, update _: ChatFeedUpdate) async {}

    func productStopped() async {
        stops += 1
    }
}

/// Waits until `condition` is true, for at most two seconds.
private func poll(_ condition: @escaping @Sendable () async -> Bool) async throws {
    let deadline = ContinuousClock.now + .seconds(2)
    while await !condition() {
        guard ContinuousClock.now < deadline else {
            Issue.record("the condition did not become true in time")
            return
        }
        try await Task.sleep(for: .milliseconds(10))
    }
}

private extension TrUAPIChatHandlerRoomTests {
    func runningWorkers() -> StubWorkerManager {
        let workers = StubWorkerManager()
        workers.publish([product: execution])
        return workers
    }

    func makeHandler(_ workers: StubWorkerManager) -> TrUAPIChatHandler {
        TrUAPIChatHandler(productId: product, workers: workers, activityStore: activity)
    }
}
