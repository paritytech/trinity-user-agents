import Foundation
import PolkadotUI
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The room screen holds the worker of its product while it shows a room, and each query holds it while it runs.
struct TrUAPIChatHandlerRoomTests {
    private let product: ProductId = "chat.dot"
    private let execution = MockProductExecution()

    @Test func anOpenHoldsTheWorker() async throws {
        let workers = runningWorkers()
        let handler = makeHandler(workers)

        try await handler.open(roomId: "source")

        #expect(workers.references.acquired == [product])
        #expect(workers.references.released.isEmpty)
        #expect(execution.publishedChatActions.isEmpty)

        await handler.dispose()
    }

    @Test func aCloseReleasesTheWorker() async throws {
        let workers = runningWorkers()
        let handler = makeHandler(workers)

        try await handler.open(roomId: "source")
        await handler.close(roomId: "source")

        #expect(execution.publishedChatActions.isEmpty)
        #expect(workers.references.log == ["acquire \(product)", "release \(product)"])

        await handler.dispose()
        #expect(workers.references.released == [product])
    }

    @Test func aCloseWithoutAnOpenDoesNothing() async {
        let workers = runningWorkers()
        let handler = makeHandler(workers)

        await handler.close(roomId: "source")

        #expect(execution.publishedChatActions.isEmpty)
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
        #expect(execution.publishedChatActions.isEmpty)

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
        let action = ChatUserAction.newMessage(body: .text(text: "hi"), replyTo: nil, messageId: nil, timestamp: nil)

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
        #expect(workers.references.log == ["acquire \(product)", "release \(product)"])

        await bot.dispose()
    }
}

private extension TrUAPIChatHandlerRoomTests {
    func runningWorkers() -> StubWorkerManager {
        let workers = StubWorkerManager()
        workers.publish([product: execution])
        return workers
    }

    func makeHandler(_ workers: StubWorkerManager) -> TrUAPIChatHandler {
        TrUAPIChatHandler(productId: product, workers: workers)
    }
}
