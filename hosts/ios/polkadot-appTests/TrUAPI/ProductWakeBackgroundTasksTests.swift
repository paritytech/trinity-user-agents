import Foundation
import Testing
import Products
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// A UIKit background task for each wake hold of a product (07-LATER §10.2, O30).
@MainActor
struct ProductWakeBackgroundTasksTests {
    private let runner = RecordingBackgroundTasks()

    @Test func aHoldRunsOneTaskUntilItsDeadline() async throws {
        let tasks = ProductWakeBackgroundTasks(runner: runner)

        tasks.hold(productId: "a.dot", until: Date().addingTimeInterval(0.1))

        #expect(runner.begun == ["truapi.wake.a.dot"])
        #expect(tasks.heldProducts == ["a.dot"])
        try await waitUntil { runner.ended.count == 1 }
        #expect(tasks.heldProducts.isEmpty)
    }

    /// A new wake moves the end of the hold, and the same task continues.
    @Test func aLaterDeadlineMovesTheEndOfTheSameTask() async throws {
        let tasks = ProductWakeBackgroundTasks(runner: runner)

        tasks.hold(productId: "a.dot", until: Date().addingTimeInterval(0.1))
        tasks.hold(productId: "a.dot", until: Date().addingTimeInterval(2))
        try await Task.sleep(for: .milliseconds(300))

        #expect(runner.begun.count == 1)
        #expect(runner.ended.isEmpty)
        try await waitUntil { runner.ended.count == 1 }
    }

    @Test func anEarlierDeadlineChangesNothing() async throws {
        let tasks = ProductWakeBackgroundTasks(runner: runner)

        tasks.hold(productId: "a.dot", until: Date().addingTimeInterval(2))
        tasks.hold(productId: "a.dot", until: Date().addingTimeInterval(0.05))
        try await Task.sleep(for: .milliseconds(300))

        #expect(runner.ended.isEmpty)
        try await waitUntil { runner.ended.count == 1 }
    }

    @Test func theEndOfTheBackgroundTimeEndsTheTask() {
        let tasks = ProductWakeBackgroundTasks(runner: runner)
        tasks.hold(productId: "a.dot", until: Date().addingTimeInterval(30))

        runner.expire()

        #expect(runner.ended.count == 1)
        #expect(tasks.heldProducts.isEmpty)
    }

    @Test func eachProductHasItsOwnTask() {
        let tasks = ProductWakeBackgroundTasks(runner: runner)

        tasks.hold(productId: "a.dot", until: Date().addingTimeInterval(30))
        tasks.hold(productId: "b.dot", until: Date().addingTimeInterval(30))

        #expect(runner.begun == ["truapi.wake.a.dot", "truapi.wake.b.dot"])
        #expect(tasks.heldProducts == ["a.dot", "b.dot"])
    }

    private func waitUntil(_ condition: @escaping @MainActor () -> Bool) async throws {
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

/// The wakes of a worker manager: the core holds the worker, and a background task keeps the app running.
struct TrUAPIWorkerManagerWakeTests {
    @Test func aWakeHoldsABackgroundTaskUntilTheEndOfTheCoreHold() async throws {
        let references = StubWorkerReferences()
        let tasks = RecordingWakeTasks()
        let manager = makeManager(references: references, tasks: tasks)

        let outcome = await manager.wake("a.dot", reason: .messageInFlight(roomId: "host"))

        guard case let .held(untilMs) = outcome else {
            Issue.record("the core must hold the worker")
            return
        }
        #expect(references.log == ["wake a.dot message-in-flight host"])
        let holds = await tasks.holds
        #expect(holds.map(\.productId) == ["a.dot"])
        #expect(holds.first?.deadline == Date(timeIntervalSince1970: TimeInterval(untilMs) / 1_000))
        await manager.shutdown()
    }

    @Test func aSpentBudgetHoldsNoBackgroundTask() async throws {
        let tasks = RecordingWakeTasks()
        let manager = makeManager(references: SpentReferences(), tasks: tasks)

        let outcome = await manager.wake("a.dot", reason: .notification(roomId: "host"))

        #expect(outcome == .budgetSpent)
        #expect(await tasks.holds.isEmpty)
        await manager.shutdown()
    }

    @Test func aCoreThatIsGoneGivesNoWake() async throws {
        let tasks = RecordingWakeTasks()
        let manager = TrUAPIWorkerManager(
            builder: NeverBuilder(),
            collection: InMemoryPocketCardStore([]),
            references: { throw CancellationError() },
            backgroundTasks: tasks
        )

        #expect(await manager.wake("a.dot", reason: .productStart) == nil)
        #expect(await tasks.holds.isEmpty)
        await manager.shutdown()
    }

    private func makeManager(
        references: any TrUAPIWorkerReferencing,
        tasks: RecordingWakeTasks
    ) -> TrUAPIWorkerManager {
        TrUAPIWorkerManager(
            builder: NeverBuilder(),
            collection: InMemoryPocketCardStore([]),
            references: { references },
            backgroundTasks: tasks
        )
    }
}

/// Records the background tasks that a wake asks for.
@MainActor
final class RecordingBackgroundTasks: BackgroundTaskRunning {
    private(set) var begun: [String] = []
    private(set) var ended: [UIBackgroundTaskIdentifier] = []
    private var expirations: [() -> Void] = []

    func begin(name: String, expiration: @escaping @MainActor () -> Void) -> UIBackgroundTaskIdentifier {
        begun.append(name)
        expirations.append(expiration)
        return UIBackgroundTaskIdentifier(rawValue: begun.count)
    }

    func end(_ identifier: UIBackgroundTaskIdentifier) {
        ended.append(identifier)
    }

    /// The system ends the background time of the app.
    func expire() {
        expirations.forEach { $0() }
    }
}

@MainActor
final class RecordingWakeTasks: ProductWakeBackgroundTasking {
    struct Hold: Equatable {
        let productId: ProductId
        let deadline: Date
    }

    private(set) var holds: [Hold] = []

    nonisolated init() {}

    func hold(productId: ProductId, until deadline: Date) {
        holds.append(Hold(productId: productId, deadline: deadline))
    }
}

/// A core whose product used its notification wakes of the hour.
private final class SpentReferences: TrUAPIWorkerReferencing, @unchecked Sendable {
    func acquireWorker(productId _: ProductId) {}
    func releaseWorker(productId _: ProductId) {}
    func setWorkerKeepAlive(productId _: ProductId, keepAlive _: WorkerKeepAlive) {}
    func wakeProduct(productId _: ProductId, reason _: ChatWakeReason) throws -> ChatWakeOutcome {
        .budgetSpent
    }
}

/// A builder for a manager whose tests start no worker.
private struct NeverBuilder: TrUAPIWorkerBuilding {
    func makeRuntime(
        productId _: ProductId,
        context _: ProductWorkerContext,
        pocket _: ProductPocketHostBridge
    ) async throws -> TrUAPIWorkerRuntime {
        throw CancellationError()
    }
}
