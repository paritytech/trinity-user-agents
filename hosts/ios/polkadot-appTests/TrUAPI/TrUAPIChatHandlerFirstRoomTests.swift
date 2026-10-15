import Foundation
import AsyncExtensions
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// A product that has no room yet holds its worker for a short time at its start, so that it can make its rooms
/// (06-OPEN-ITEMS O25, 02-DESIGN §22 item 4).
struct TrUAPIChatHandlerFirstRoomTests {
    private let product: ProductId = "test.dot"
    private let execution = MockProductExecution()
    private let rooms = AsyncCurrentValueSubject<[RoomInfo]>([])

    @Test func aProductWithNoRoomHoldsItsWorkerUntilItsFirstRoomAppears() async throws {
        let workers = runningWorkers()
        let handler = makeHandler(workers, hold: .init(limit: .seconds(10), settle: .milliseconds(50)))

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        try await waitUntil { workers.references.acquired.count == 1 }
        #expect(workers.references.released.isEmpty)

        rooms.send([Self.room("host")])
        try await waitUntil { workers.references.released.count == 1 }

        #expect(workers.references.log == ["acquire \(product)", "release \(product)"])

        await handler.dispose()
        #expect(workers.references.released.count == 1)
    }

    @Test func aProductWithARoomGetsNoHold() async throws {
        rooms.send([Self.room("host")])
        let workers = runningWorkers()
        let handler = makeHandler(workers, hold: .init(limit: .seconds(10), settle: .milliseconds(50)))

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        try await Task.sleep(for: .milliseconds(200))

        #expect(workers.references.acquired.isEmpty)

        await handler.dispose()
    }

    @Test func aProductWhoseRoomsAreNotKnownGetsNoHold() async throws {
        let workers = runningWorkers()
        let handler = TrUAPIChatHandler(
            productId: product,
            workers: workers,
            firstRoomHold: .init(limit: .seconds(10), settle: .milliseconds(50))
        )

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        try await Task.sleep(for: .milliseconds(200))

        #expect(workers.references.acquired.isEmpty)

        await handler.dispose()
    }

    @Test func theHoldEndsAtItsLimitWhenNoRoomAppears() async throws {
        let workers = runningWorkers()
        let handler = makeHandler(workers, hold: .init(limit: .milliseconds(150), settle: .milliseconds(50)))

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        try await waitUntil { workers.references.released.count == 1 }

        #expect(workers.references.log == ["acquire \(product)", "release \(product)"])

        await handler.dispose()
    }

    @Test func aWorkerThatDoesNotStartGivesTheRequestBack() async throws {
        let workers = StubWorkerManager(startupWindow: .milliseconds(50))
        let handler = makeHandler(workers, hold: .init(limit: .seconds(10), settle: .milliseconds(50)))

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        try await waitUntil { workers.references.released.count == 1 }

        #expect(workers.references.log == ["acquire \(product)", "release \(product)"])

        await handler.dispose()
    }

    @Test func aDisposeDuringTheHoldGivesTheRequestBackOnce() async throws {
        let workers = runningWorkers()
        let handler = makeHandler(workers, hold: .init(limit: .seconds(10), settle: .milliseconds(50)))

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        try await waitUntil { workers.references.acquired.count == 1 }

        await handler.dispose()

        #expect(workers.references.log == ["acquire \(product)", "release \(product)"])
    }

    @Test func noHoldWhenTheHoldIsOff() async throws {
        let workers = runningWorkers()
        let handler = makeHandler(workers, hold: nil)

        try await handler.start(messagingSupport: .init(bot: nil, context: nil))
        try await Task.sleep(for: .milliseconds(200))

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

    func makeHandler(_ workers: StubWorkerManager, hold: ProductFirstRoomHold.Limits?) -> TrUAPIChatHandler {
        TrUAPIChatHandler(
            productId: product,
            workers: workers,
            firstRoomHold: hold,
            roomsOfProduct: { [rooms] _ in rooms.eraseToAnyAsyncSequence() }
        )
    }

    /// Waits until `condition` is true, for at most two seconds.
    func waitUntil(_ condition: @escaping () -> Bool) async throws {
        let deadline = ContinuousClock.now + .seconds(2)
        while !condition() {
            guard ContinuousClock.now < deadline else {
                Issue.record("the condition did not become true in time")
                return
            }
            try await Task.sleep(for: .milliseconds(10))
        }
    }
}
