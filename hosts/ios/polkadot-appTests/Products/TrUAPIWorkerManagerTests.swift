import Foundation
import IssueMonitoring
import Products
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
import TrUAPIHost
@testable import polkadot_app

private let gameProductId = try! ProductId("game.paseo")
private let barrierProductId = try! ProductId("barrier.paseo")
private let stalledProductId = try! ProductId("stalled.paseo")

/// The core counts worker references and reports only the transitions across
/// zero, so the manager must be exact: a worker left running burns a web
/// view, and one left half-booted swallows every later start.
struct TrUAPIWorkerManagerTests {
    @Test
    func startsTheWorkerOnTheFirstDemand() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder)

        manager.demandChanged(productId: gameProductId, transition: .start)
        try await execution(of: gameProductId, in: manager)

        #expect(builder.built == [gameProductId])
    }

    /// The core reports only transitions across zero, but a stop and a start can
    /// overtake each other; a second start while one is running must not open a
    /// second execution for the same product.
    @Test
    func doesNotStartASecondWorkerForTheSameProduct() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder)

        manager.demandChanged(productId: gameProductId, transition: .start)
        try await execution(of: gameProductId, in: manager)
        manager.demandChanged(productId: gameProductId, transition: .start)
        try await barrier(manager)

        #expect(builder.built == [gameProductId, barrierProductId])
    }

    @Test
    func stopsTheWorkerAndForgetsItsExecution() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder)

        manager.demandChanged(productId: gameProductId, transition: .start)
        try await execution(of: gameProductId, in: manager)
        manager.demandChanged(productId: gameProductId, transition: .stop)

        try await noExecution(of: gameProductId, in: manager)
    }

    /// A worker that is only forgotten keeps its headless web view and chain
    /// connections running for the rest of the session.
    @Test
    func disposesTheWorkerItStops() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder)

        manager.demandChanged(productId: gameProductId, transition: .start)
        try await execution(of: gameProductId, in: manager)
        let running = try #require(builder.executions[gameProductId])
        manager.demandChanged(productId: gameProductId, transition: .stop)
        try await withTimeout(.hangGuard) { await running.awaitClose() }

        #expect(running.closeCallCount == 1)
        #expect(running.stopWsBridgeCallCount == 1)
    }

    /// iOS kills a worker's content process while the app is in the background,
    /// and the script dies with it. The core keeps counting the references that
    /// wanted the worker, so it never sends another start: unless the manager
    /// boots a fresh worker itself, chat and Pocket faces for the product stay
    /// dead until the app restarts.
    @Test
    func bootsAFreshWorkerWhenItsEngineDies() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder)

        manager.demandChanged(productId: gameProductId, transition: .start)
        try await execution(of: gameProductId, in: manager)
        let dead = try #require(builder.executions[gameProductId])
        try #require(builder.engines[gameProductId]).terminate()
        try await withTimeout(.hangGuard) { await dead.awaitClose() }
        let rebooted = try await execution(of: gameProductId, in: manager)

        #expect(rebooted !== dead)
        #expect(builder.built == [gameProductId, gameProductId])
    }

    /// An engine of a worker that was already stopped can still die afterwards.
    /// Rebooting it would run a worker nobody holds a reference to, and the core
    /// would never send the stop that clears it.
    @Test
    func doesNotRebootAWorkerThatWasStopped() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder)

        manager.demandChanged(productId: gameProductId, transition: .start)
        try await execution(of: gameProductId, in: manager)
        let engine = try #require(builder.engines[gameProductId])
        manager.demandChanged(productId: gameProductId, transition: .stop)
        try await noExecution(of: gameProductId, in: manager)
        engine.terminate()
        try await barrier(manager)

        #expect(builder.built == [gameProductId, barrierProductId])
        #expect(manager.currentExecution(of: gameProductId) == nil)
    }

    /// A release of the worker's archive restarts it, so the next boot reads
    /// the newly applied archive.
    @Test
    func reloadBootsAFreshWorker() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder)

        manager.demandChanged(productId: gameProductId, transition: .start)
        try await execution(of: gameProductId, in: manager)
        let previous = try #require(builder.executions[gameProductId])
        await manager.reload(productId: gameProductId)
        try await withTimeout(.hangGuard) { await previous.awaitClose() }
        let reloaded = try await execution(of: gameProductId, in: manager)

        #expect(reloaded !== previous)
        #expect(builder.built == [gameProductId, gameProductId])
    }

    /// A stop the ledger sends while a reload is disposing the worker must
    /// still land: a restart that overtook it would run a worker nobody holds,
    /// and the core never sends another stop to clear it.
    @Test
    func aStopSentDuringAReloadLeavesNoWorker() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder)
        let disposing = AsyncStream<Void>.makeStream()
        let releaseDispose = AsyncStream<Void>.makeStream()

        manager.demandChanged(productId: gameProductId, transition: .start)
        try await execution(of: gameProductId, in: manager)
        try #require(builder.engines[gameProductId]).onDestroy = {
            disposing.continuation.yield()
            for await _ in releaseDispose.stream {}
        }
        let reloading = Task { await manager.reload(productId: gameProductId) }
        try await withTimeout(.hangGuard) {
            for await _ in disposing.stream {
                return
            }
        }
        manager.demandChanged(productId: gameProductId, transition: .stop)
        releaseDispose.continuation.finish()
        await reloading.value
        try await barrier(manager)

        try await noExecution(of: gameProductId, in: manager)
        #expect(manager.currentExecution(of: gameProductId) == nil)
    }

    @Test
    func reloadDoesNotStartAWorkerThatIsNotRunning() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder)

        await manager.reload(productId: gameProductId)
        try await barrier(manager)

        #expect(builder.built == [barrierProductId])
    }

    /// A worker whose engine fails once it is already in the running map must
    /// leave nothing behind: the core keeps counting the reference the card
    /// holds, so no further stop arrives to clear a half-started entry, and
    /// every later start would be swallowed by the entry that is still there.
    @Test
    func leavesNothingBehindWhenTheEngineFailsToBoot() async throws {
        let logger = MockLogger()
        let builder = StubWorkerBuilder(engineFails: true)
        let manager = makeManager(builder: builder, logger: logger)

        manager.demandChanged(productId: gameProductId, transition: .start)
        try await logger.waitForError(containing: "failed to start")

        #expect(manager.currentExecution(of: gameProductId) == nil)

        builder.engineFails = false
        manager.demandChanged(productId: gameProductId, transition: .start)
        try await execution(of: gameProductId, in: manager)
    }

    /// A product with no worker cannot be built at all, which must be as clean
    /// a failure as one that breaks while booting.
    @Test
    func leavesNothingBehindWhenTheWorkerCannotBeBuilt() async throws {
        let logger = MockLogger()
        let builder = StubWorkerBuilder(cannotBuild: true)
        let manager = makeManager(builder: builder, logger: logger)

        manager.demandChanged(productId: gameProductId, transition: .start)
        try await logger.waitForError(containing: "failed to start")

        #expect(manager.currentExecution(of: gameProductId) == nil)

        builder.cannotBuild = false
        manager.demandChanged(productId: gameProductId, transition: .start)
        try await execution(of: gameProductId, in: manager)
    }

    /// A stop for a product that never started is what the core sends when a
    /// card's worker was refused, and it must not tear anything else down.
    @Test
    func ignoresAStopForAWorkerThatNeverStarted() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder)

        manager.demandChanged(productId: gameProductId, transition: .start)
        try await execution(of: gameProductId, in: manager)
        manager.demandChanged(productId: try! ProductId("other.paseo"), transition: .stop)
        try await barrier(manager)

        #expect(manager.currentExecution(of: gameProductId) != nil)
    }

    /// Building a worker fetches its archive, which is exactly when two cards
    /// for one product scroll into view together. Actors are reentrant, so both
    /// starts reach the guard while the first is still waiting on it: two
    /// workers for one product is two headless web views, and only the last is
    /// ever stopped.
    @Test
    func doesNotStartASecondWorkerWhenTwoDemandsOverlap() async throws {
        let builder = StubWorkerBuilder(buildDelay: .milliseconds(30))
        let manager = makeManager(builder: builder)

        manager.demandChanged(productId: gameProductId, transition: .start)
        manager.demandChanged(productId: gameProductId, transition: .start)
        await builder.clock.waitForSleep(for: .milliseconds(30))
        try await barrier(manager)
        await builder.clock.resumeSleep(for: .milliseconds(30))
        try await execution(of: gameProductId, in: manager)

        #expect(builder.built.filter { $0 == gameProductId }.count == 1)
    }

    /// A stop that lands while the worker is still being built finds nothing to
    /// remove, and the boot then publishes an execution no stop can reach: the
    /// web view and its chain connections run for the rest of the session.
    @Test
    func disposesAWorkerThatWasStoppedWhileItWasStillBooting() async throws {
        let builder = StubWorkerBuilder(buildDelay: .milliseconds(80))
        let manager = makeManager(builder: builder)

        manager.demandChanged(productId: gameProductId, transition: .start)
        await builder.clock.waitForSleep(for: .milliseconds(80))
        let booting = try #require(builder.executions[gameProductId])
        manager.demandChanged(productId: gameProductId, transition: .stop)
        try await barrier(manager)
        await builder.clock.resumeSleep(for: .milliseconds(80))
        try await withTimeout(.hangGuard) { await booting.awaitClose() }

        #expect(manager.currentExecution(of: gameProductId) == nil)
        #expect(booting.closeCallCount == 1)
    }

    /// The ledger delivers its transitions in order, so the manager has to
    /// apply them in order. A start, a stop and a start leave the worker
    /// running; applied out of order the pair of starts collapses into one and
    /// the stop lands last, leaving the card with no worker at all.
    @Test
    func appliesABurstOfTransitionsInTheOrderTheyArrive() async throws {
        let builder = StubWorkerBuilder(buildDelay: .milliseconds(40))
        let manager = makeManager(builder: builder)

        manager.demandChanged(productId: gameProductId, transition: .start)
        manager.demandChanged(productId: gameProductId, transition: .stop)
        manager.demandChanged(productId: gameProductId, transition: .start)

        await builder.clock.waitForSleeps(for: .milliseconds(40), count: 2)
        await builder.clock.resumeSleeps(for: .milliseconds(40), count: 2)

        try await execution(of: gameProductId, in: manager)
    }

    /// A boot that fails after a stop has already cleared it must not take the
    /// boot that replaced it down too. The core counts the card's reference and
    /// reports only transitions across zero, so a start swallowed here never
    /// comes again and the card is left on whatever it last drew.
    @Test
    func aFailedBootDoesNotClearTheBootThatReplacedIt() async throws {
        let builder = StubWorkerBuilder(firstStartFailsAfter: .milliseconds(300))
        let manager = makeManager(builder: builder)

        manager.demandChanged(productId: gameProductId, transition: .start)
        await builder.clock.waitForSleep(for: .milliseconds(300))
        let first = try #require(builder.executions[gameProductId])

        manager.demandChanged(productId: gameProductId, transition: .stop)
        manager.demandChanged(productId: gameProductId, transition: .start)
        let booted = try await execution(of: gameProductId, in: manager)
        let published = try #require(booted as? MockProductExecution)
        #expect(published !== first)

        await builder.clock.resumeSleep(for: .milliseconds(300))
        try await barrier(manager)

        let current = manager.currentExecution(of: gameProductId) as? MockProductExecution
        #expect(current === published)
        #expect(published.closeCallCount == 0)
    }

    /// Every product's transitions arrive down one queue, and building a worker
    /// fetches its archive over the network. A product whose archive never
    /// comes back must not leave every other product's card without a worker
    /// for as long as the fetch hangs.
    @Test
    func aStalledBootDoesNotHoldUpAnotherProduct() async throws {
        let builder = StubWorkerBuilder(stalledProduct: stalledProductId)
        defer { builder.releaseTheStalledBoot() }
        let manager = makeManager(builder: builder)

        manager.demandChanged(productId: stalledProductId, transition: .start)
        manager.demandChanged(productId: gameProductId, transition: .start)
        try await execution(of: gameProductId, in: manager)

        #expect(manager.currentExecution(of: stalledProductId) == nil)
    }

    /// Each activation gets its own host session, held from its boot until it stops, and the boot is
    /// reported with the phases it went through.
    @Test
    func aWorkerThatComesUpReportsItsBootUnderItsOwnSession() async throws {
        let recorder = RecordingHostEventRecorder()
        let manager = makeManager(builder: StubWorkerBuilder(), telemetry: HostLoadTelemetry(recorder: recorder))

        manager.demandChanged(productId: gameProductId, transition: .start)
        try await execution(of: gameProductId, in: manager)
        try await recorder.waitForEvents(count: 1)

        let boot = try #require(recorder.events.first)
        let session = try #require(manager.hostSessionId(of: gameProductId))
        #expect(boot.name == "host.worker.boot")
        #expect(boot.tags["outcome"] == "ok")
        #expect(boot.tags["product.id"] == "game.paseo")
        #expect(boot.tags["host.session_id"] == session.uuidString.lowercased())
        #expect(boot.extras["webview_create_ms"] != nil)
        #expect(boot.extras["blank_ready_ms"] != nil)
        #expect(boot.extras["execution_open_ms"] != nil)

        manager.demandChanged(productId: gameProductId, transition: .stop)
        try await noExecution(of: gameProductId, in: manager)
        #expect(manager.hostSessionId(of: gameProductId) == nil)
    }

    @Test
    func aProductThatPublishesNoWorkerReportsNoWorker() async throws {
        let recorder = RecordingHostEventRecorder()
        let builder = StubWorkerBuilder(cannotBuild: true)
        let manager = makeManager(builder: builder, telemetry: HostLoadTelemetry(recorder: recorder))

        manager.demandChanged(productId: gameProductId, transition: .start)
        try await recorder.waitForEvents(count: 1)

        #expect(recorder.events.map { $0.tags["outcome"] } == ["no_worker"])
    }

    @Test
    func aWorkerWhoseEngineFailsReportsAnError() async throws {
        let recorder = RecordingHostEventRecorder()
        let builder = StubWorkerBuilder(engineFails: true)
        let manager = makeManager(builder: builder, telemetry: HostLoadTelemetry(recorder: recorder))

        manager.demandChanged(productId: gameProductId, transition: .start)
        try await recorder.waitForEvents(count: 1)

        #expect(recorder.events.map { $0.tags["outcome"] } == ["error"])
        #expect(recorder.events.first?.extras["blank_ready_ms"] == nil)
    }

    @Test
    func aDisabledTelemetryStartsNoSession() async throws {
        let manager = makeManager(builder: StubWorkerBuilder())

        manager.demandChanged(productId: gameProductId, transition: .start)
        try await execution(of: gameProductId, in: manager)

        #expect(manager.hostSessionId(of: gameProductId) == nil)
    }
}

// MARK: - Fixtures

/// Waits for the product's worker to be published, failing rather than hanging if it never is.
@discardableResult
private func execution(
    of productId: ProductId,
    in manager: TrUAPIWorkerManager
) async throws -> TrUAPIProductExecutionProtocol {
    try await withTimeout(.hangGuard) {
        for try await execution in manager.executions(of: productId) {
            if let execution { return execution }
        }
        throw TrUAPIWorkerError.noWorker(productId)
    }
}

/// Waits for the product's worker to be cleared, failing rather than hanging if it never is.
private func noExecution(of productId: ProductId, in manager: TrUAPIWorkerManager) async throws {
    struct StreamEnded: Error {}
    try await withTimeout(.hangGuard) {
        for try await execution in manager.executions(of: productId) where execution == nil {
            return
        }
        throw StreamEnded()
    }
}

/// Transitions apply in order, so once the barrier's worker is up every
/// transition sent before it has been applied.
private func barrier(_ manager: TrUAPIWorkerManager) async throws {
    manager.demandChanged(productId: barrierProductId, transition: .start)
    try await execution(of: barrierProductId, in: manager)
}

private func makeManager(
    builder: any TrUAPIWorkerBuilding,
    references: StubWorkerReferences = StubWorkerReferences(),
    startupWindow: Duration = .seconds(30),
    telemetry: HostLoadTelemetry = .disabled,
    logger: MockLogger = MockLogger()
) -> TrUAPIWorkerManager {
    TrUAPIWorkerManager(
        builder: builder,
        references: { references },
        startupWindow: startupWindow,
        telemetry: telemetry,
        logger: logger
    )
}
