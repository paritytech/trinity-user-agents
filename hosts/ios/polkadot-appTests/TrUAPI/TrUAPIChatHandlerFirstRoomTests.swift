import Foundation
import AsyncExtensions
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// A product that has no room yet gets one wake at its start, so that it can make its rooms (W2 of 07-LATER §10.2,
/// 06-OPEN-ITEMS O25). The core holds the worker for the wake and releases it, so the handler takes no request.
struct TrUAPIChatHandlerFirstRoomTests {
    private let product: ProductId = "chat.dot"
    private let execution = MockProductExecution()
    private let rooms = AsyncCurrentValueSubject<[RoomInfo]>([])

    @Test func aProductWithNoRoomGetsOneStartWake() async throws {
        let workers = runningWorkers()
        let handler = makeHandler(workers, wakesAtStart: true)

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        try await waitUntil { workers.wakes == [.productStart] }

        rooms.send([Self.room("host")])
        try await Task.sleep(for: .milliseconds(100))

        #expect(workers.wakes == [.productStart])
        #expect(workers.references.log.isEmpty)

        await handler.dispose()
        #expect(workers.references.log.isEmpty)
    }

    @Test func aProductWithARoomGetsNoWake() async throws {
        rooms.send([Self.room("host")])
        let workers = runningWorkers()
        let handler = makeHandler(workers, wakesAtStart: true)

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        try await Task.sleep(for: .milliseconds(200))

        #expect(workers.wakes.isEmpty)
        #expect(workers.references.acquired.isEmpty)

        await handler.dispose()
    }

    @Test func aProductWhoseRoomsAreNotKnownGetsNoWake() async throws {
        let workers = runningWorkers()
        let handler = TrUAPIChatHandler(productId: product, workers: workers, wakesAtStart: true)

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        try await Task.sleep(for: .milliseconds(200))

        #expect(workers.wakes.isEmpty)
        #expect(workers.references.acquired.isEmpty)

        await handler.dispose()
    }

    @Test func aDisposeBeforeTheRoomsAreKnownWakesNothing() async throws {
        let workers = runningWorkers()
        let pending = AsyncStream<[RoomInfo]>.makeStream()
        let handler = TrUAPIChatHandler(
            productId: product,
            workers: workers,
            wakesAtStart: true,
            roomsOfProduct: { _ in pending.stream.eraseToAnyAsyncSequence() }
        )

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        await handler.dispose()
        pending.continuation.yield([])
        pending.continuation.finish()
        try await Task.sleep(for: .milliseconds(100))

        #expect(workers.wakes.isEmpty)
    }

    /// Any other product holds its worker from the start of its bot to its dispose, as at the merge base, so its
    /// bot posts and its cards draw while no room screen is open. It gets no start wake.
    @Test func anotherProductHoldsItsWorkerForTheSession() async throws {
        let game: ProductId = "game.dot"
        let workers = StubWorkerManager()
        workers.publish([game: execution])
        let handler = TrUAPIChatHandler(productId: game, workers: workers, wakesAtStart: true)

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        #expect(workers.references.acquired == [game])
        #expect(workers.wakes.isEmpty)

        await handler.dispose()
        await handler.dispose()
        #expect(workers.references.released == [game])
    }

    @Test func noWakeWhenTheWakeIsOff() async throws {
        let workers = runningWorkers()
        let handler = makeHandler(workers, wakesAtStart: false)

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        try await Task.sleep(for: .milliseconds(200))

        #expect(workers.wakes.isEmpty)
        #expect(workers.references.acquired.isEmpty)

        await handler.dispose()
    }
}

private extension TrUAPIChatHandlerFirstRoomTests {
    static func room(_ roomId: String) -> RoomInfo {
        RoomInfo(roomId: roomId, name: roomId, icon: nil, participation: .roomHost)
    }

    func runningWorkers() -> StubWorkerManager {
        let workers = StubWorkerManager()
        workers.publish([product: execution])
        return workers
    }

    func makeHandler(_ workers: StubWorkerManager, wakesAtStart: Bool) -> TrUAPIChatHandler {
        TrUAPIChatHandler(
            productId: product,
            workers: workers,
            wakesAtStart: wakesAtStart,
            roomsOfProduct: { [rooms] _ in rooms.eraseToAnyAsyncSequence() }
        )
    }

    /// Waits until `condition` is true, for at most ten seconds.
    func waitUntil(_ condition: @escaping () -> Bool) async throws {
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
