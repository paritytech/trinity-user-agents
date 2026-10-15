import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// A user message, a user action or an event of a room wakes the worker for 30 seconds, so the product answers
/// after the user closes the room (W1 of 07-LATER §10.2, scenario S69).
struct TrUAPIChatHandlerWakeTests {
    private let product: ProductId = "test.dot"

    @Test func aUserMessageWakesTheWorkerForItsRoom() async throws {
        let execution = MockProductExecution()
        let workers = runningWorkers(execution)
        let handler = makeHandler(workers)

        try await handler.onUserMessage(text: "hi", roomId: "host")

        #expect(workers.wakes == [.messageInFlight(roomId: "host")])
        #expect(execution.publishedChatActions.map(\.roomId) == ["host"])
        #expect(workers.references.log.isEmpty)
    }

    @Test func eachUserEventWakesTheWorker() async throws {
        let execution = MockProductExecution()
        let workers = runningWorkers(execution)
        let handler = makeHandler(workers)

        try await handler.publishUserEvent(.seen(messageId: "m1"), roomId: "host")
        _ = await handler.dispatchCommand(Chat.Command(name: "help", arguments: ""), roomId: "host")
        await handler.dispatchActionTrigger(roomId: "host", messageId: "m1", actionId: "a1")

        #expect(workers.wakes == Array(repeating: .messageInFlight(roomId: "host"), count: 3))
        #expect(execution.publishedChatEvents.count == 3)
    }

    @Test func aUserActionWakesTheWorkerBeforeItsQuery() async throws {
        let execution = MockProductExecution()
        let workers = runningWorkers(execution)
        let handler = makeHandler(workers)

        _ = try? await handler.userAction(ProductChatUserActionRequest(
            roomId: "source",
            action: .retry(messageId: "m1")
        ))

        #expect(workers.wakes == [.messageInFlight(roomId: "source")])
    }

    /// A message to a product whose worker does not run starts the worker through the wake, and goes out when the
    /// worker is up: for example a message after the user closed every room.
    @Test func aMessageWaitsForTheWorkerThatTheWakeStarts() async throws {
        let execution = MockProductExecution()
        let workers = StubWorkerManager()
        let handler = makeHandler(workers, window: .seconds(2))

        let send = Task { try await handler.onUserMessage(text: "hi", roomId: "host") }
        try await Task.sleep(for: .milliseconds(100))
        workers.publish([product: execution])
        try await send.value

        #expect(workers.wakes == [.messageInFlight(roomId: "host")])
        #expect(execution.publishedChatActions.map(\.roomId) == ["host"])
    }

    @Test func aWorkerThatDoesNotStartInTheWindowFailsTheMessage() async throws {
        let workers = StubWorkerManager()
        let handler = makeHandler(workers, window: .milliseconds(100))

        await #expect(throws: TrUAPIChatHandler.ChatSeamError.notStarted) {
            try await handler.onUserMessage(text: "hi", roomId: "host")
        }
    }

    /// When the core cannot wake the product, nothing starts the worker, so the message fails at once.
    @Test func aWakeThatFailsDoesNotWaitForAWorker() async throws {
        let workers = StubWorkerManager()
        workers.wakeOutcome = nil
        let handler = makeHandler(workers, window: .seconds(30))

        let start = ContinuousClock.now
        await #expect(throws: TrUAPIChatHandler.ChatSeamError.notStarted) {
            try await handler.onUserMessage(text: "hi", roomId: "host")
        }
        #expect(ContinuousClock.now - start < .seconds(5))
    }
}

private extension TrUAPIChatHandlerWakeTests {
    func runningWorkers(_ execution: MockProductExecution) -> StubWorkerManager {
        let workers = StubWorkerManager()
        workers.publish([product: execution])
        return workers
    }

    func makeHandler(_ workers: StubWorkerManager, window: Duration = .seconds(1)) -> TrUAPIChatHandler {
        TrUAPIChatHandler(productId: product, workers: workers, messageStartupWindow: window, wakesAtStart: false)
    }

    func poll(_ condition: @escaping () -> Bool) async throws {
        let deadline = ContinuousClock.now + .seconds(10)
        while !condition() {
            guard ContinuousClock.now < deadline else {
                Issue.record("the condition did not become true in time")
                return
            }
            try await Task.sleep(for: .milliseconds(10))
        }
    }
}
