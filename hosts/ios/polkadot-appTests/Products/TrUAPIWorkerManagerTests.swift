import Foundation
import Products
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The core counts worker references and reports only the transitions across
/// zero, so the manager must be exact: a worker left running burns a web
/// view, and one left half-booted swallows every later start.
struct TrUAPIWorkerManagerTests {
    @Test
    func startsTheWorkerOnTheFirstDemand() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await execution(of: "game.paseo", in: manager)

        #expect(builder.built == ["game.paseo"])
    }

    /// The core reports only transitions across zero, but a stop and a start can
    /// overtake each other; a second start while one is running must not open a
    /// second execution for the same product.
    @Test
    func doesNotStartASecondWorkerForTheSameProduct() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await execution(of: "game.paseo", in: manager)
        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await barrier(manager)

        #expect(builder.built == ["game.paseo", "barrier.paseo"])
    }

    @Test
    func stopsTheWorkerAndForgetsItsExecution() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await execution(of: "game.paseo", in: manager)
        manager.demandChanged(productId: "game.paseo", transition: .stop)

        try await noExecution(of: "game.paseo", in: manager)
    }

    /// A worker that is only forgotten keeps its headless web view and chain
    /// connections running for the rest of the session.
    @Test
    func disposesTheWorkerItStops() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await execution(of: "game.paseo", in: manager)
        let running = try #require(builder.executions["game.paseo"])
        manager.demandChanged(productId: "game.paseo", transition: .stop)
        try await withTimeout(.hangGuard) { await running.awaitClose() }

        #expect(running.closeCallCount == 1)
        #expect(running.stopWsBridgeCallCount == 1)
    }

    /// A worker whose engine fails once it is already in the running map must
    /// leave nothing behind: the core keeps counting the reference the card
    /// holds, so no further stop arrives to clear a half-started entry, and
    /// every later start would be swallowed by the entry that is still there.
    @Test
    func leavesNothingBehindWhenTheEngineFailsToBoot() async throws {
        let logger = MockLogger()
        let builder = StubWorkerBuilder(engineFails: true)
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]), logger: logger)

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await logger.waitForError(containing: "failed to start")

        #expect(manager.currentExecution(of: "game.paseo") == nil)

        builder.engineFails = false
        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await execution(of: "game.paseo", in: manager)
    }

    /// A product with no worker cannot be built at all, which must be as clean
    /// a failure as one that breaks while booting.
    @Test
    func leavesNothingBehindWhenTheWorkerCannotBeBuilt() async throws {
        let logger = MockLogger()
        let builder = StubWorkerBuilder(cannotBuild: true)
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]), logger: logger)

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await logger.waitForError(containing: "failed to start")

        #expect(manager.currentExecution(of: "game.paseo") == nil)

        builder.cannotBuild = false
        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await execution(of: "game.paseo", in: manager)
    }

    /// A stop for a product that never started is what the core sends when a
    /// card's worker was refused, and it must not tear anything else down.
    @Test
    func ignoresAStopForAWorkerThatNeverStarted() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await execution(of: "game.paseo", in: manager)
        manager.demandChanged(productId: "other.paseo", transition: .stop)
        try await barrier(manager)

        #expect(manager.currentExecution(of: "game.paseo") != nil)
    }

    /// The worker's card list is served from the bridge's snapshot, and the
    /// script that subscribes to it comes up inside `runtime.start()`. A
    /// snapshot filled after that point leaves the product reading an empty
    /// Pocket for the whole life of its worker.
    @Test
    func fillsTheCardListBeforeTheWorkersScriptComesUp() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await execution(of: "game.paseo", in: manager)

        #expect(builder.cardsWhenTheEngineBooted.map(\.cardId) == ["loyalty"])
    }

    /// Republishing reads the execution back out of the published map, so a
    /// republish that ran before the execution was published reached nobody,
    /// and the bridge, whose snapshot had already moved, never sent another.
    @Test
    func tellsTheCoreWhatTheWorkerHoldsOnceItsExecutionIsPublished() async throws {
        let builder = StubWorkerBuilder()
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await execution(of: "game.paseo", in: manager)
        try await barrier(manager)

        #expect(builder.executions["game.paseo"]?.pocketCardNotifications.last?.map(\.cardId) == ["loyalty"])
    }

    /// Building a worker fetches its archive, which is exactly when two cards
    /// for one product scroll into view together. Actors are reentrant, so both
    /// starts reach the guard while the first is still waiting on it: two
    /// workers for one product is two headless web views, and only the last is
    /// ever stopped.
    @Test
    func doesNotStartASecondWorkerWhenTwoDemandsOverlap() async throws {
        let builder = StubWorkerBuilder(buildDelay: .milliseconds(30))
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        manager.demandChanged(productId: "game.paseo", transition: .start)
        await builder.clock.waitForSleep(for: .milliseconds(30))
        try await barrier(manager)
        await builder.clock.resumeSleep(for: .milliseconds(30))
        try await execution(of: "game.paseo", in: manager)

        #expect(builder.built.filter { $0 == "game.paseo" }.count == 1)
    }

    /// A stop that lands while the worker is still being built finds nothing to
    /// remove, and the boot then publishes an execution no stop can reach: the
    /// web view and its chain connections run for the rest of the session.
    @Test
    func disposesAWorkerThatWasStoppedWhileItWasStillBooting() async throws {
        let builder = StubWorkerBuilder(buildDelay: .milliseconds(80))
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        await builder.clock.waitForSleep(for: .milliseconds(80))
        let booting = try #require(builder.executions["game.paseo"])
        manager.demandChanged(productId: "game.paseo", transition: .stop)
        try await barrier(manager)
        await builder.clock.resumeSleep(for: .milliseconds(80))
        try await withTimeout(.hangGuard) { await booting.awaitClose() }

        #expect(manager.currentExecution(of: "game.paseo") == nil)
        #expect(booting.closeCallCount == 1)
    }

    /// The ledger delivers its transitions in order, so the manager has to
    /// apply them in order. A start, a stop and a start leave the worker
    /// running; applied out of order the pair of starts collapses into one and
    /// the stop lands last, leaving the card with no worker at all.
    @Test
    func appliesABurstOfTransitionsInTheOrderTheyArrive() async throws {
        let builder = StubWorkerBuilder(buildDelay: .milliseconds(40))
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        manager.demandChanged(productId: "game.paseo", transition: .stop)
        manager.demandChanged(productId: "game.paseo", transition: .start)

        await builder.clock.waitForSleeps(for: .milliseconds(40), count: 2)
        await builder.clock.resumeSleeps(for: .milliseconds(40), count: 2)

        try await execution(of: "game.paseo", in: manager)
    }

    /// A boot that fails after a stop has already cleared it must not take the
    /// boot that replaced it down too. The core counts the card's reference and
    /// reports only transitions across zero, so a start swallowed here never
    /// comes again and the card is left on whatever it last drew.
    @Test
    func aFailedBootDoesNotClearTheBootThatReplacedIt() async throws {
        let builder = StubWorkerBuilder(firstStartFailsAfter: .milliseconds(300))
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        await builder.clock.waitForSleep(for: .milliseconds(300))
        let first = try #require(builder.executions["game.paseo"])

        manager.demandChanged(productId: "game.paseo", transition: .stop)
        manager.demandChanged(productId: "game.paseo", transition: .start)
        let booted = try await execution(of: "game.paseo", in: manager)
        let published = try #require(booted as? MockProductExecution)
        #expect(published !== first)

        await builder.clock.resumeSleep(for: .milliseconds(300))
        try await barrier(manager)

        let current = manager.currentExecution(of: "game.paseo") as? MockProductExecution
        #expect(current === published)
        #expect(published.closeCallCount == 0)
    }

    /// Every product's transitions arrive down one queue, and building a worker
    /// fetches its archive over the network. A product whose archive never
    /// comes back must not leave every other product's card without a worker
    /// for as long as the fetch hangs.
    @Test
    func aStalledBootDoesNotHoldUpAnotherProduct() async throws {
        let builder = StubWorkerBuilder(stalledProduct: "stalled.paseo")
        defer { builder.releaseTheStalledBoot() }
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "stalled.paseo", transition: .start)
        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await execution(of: "game.paseo", in: manager)

        #expect(manager.currentExecution(of: "stalled.paseo") == nil)
    }

    /// A worker already running is told about a card added later because its
    /// bridge follows storage. Nothing announces the write, so a writer that
    /// forgot to say it wrote cannot leave a product's own list stale.
    @Test
    func tellsARunningWorkerAboutACardAddedLater() async throws {
        let builder = StubWorkerBuilder()
        let collection = InMemoryPocketCardStore([loyalty])
        let manager = makeManager(builder: builder, collection: collection)

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await execution(of: "game.paseo", in: manager)
        let running = try #require(builder.executions["game.paseo"])
        #expect(try builder.bridge?.listCards().map(\.cardId) == ["loyalty"])

        try await collection.add(streak, face: .nil)
        // The add can land before the bridge starts republishing, in which case
        // the opening list already holds it and no second list is sent.
        let told = try await withTimeout(.hangGuard) { () async -> [String] in
            while let cards = await running.awaitPocketCardNotification() {
                let ids = cards.map(\.cardId)
                if ids.contains("streak") { return ids.sorted() }
            }
            return []
        }

        #expect(told == ["loyalty", "streak"])
        #expect(try builder.bridge?.listCards().map(\.cardId).sorted() == ["loyalty", "streak"])
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardEntry(
    key: PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty")),
    title: "Loyalty",
    privileged: false
)

private let streak = PocketCardEntry(
    key: PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "streak")),
    title: "Streak",
    privileged: false
)

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
    manager.demandChanged(productId: "barrier.paseo", transition: .start)
    try await execution(of: "barrier.paseo", in: manager)
}

private func collectionHolding(_ cards: [PocketCardEntry]) -> any PocketCardStore {
    InMemoryPocketCardStore(cards)
}

private func makeManager(
    builder: any TrUAPIWorkerBuilding,
    collection: any PocketCardStore,
    references: StubWorkerReferences = StubWorkerReferences(),
    startupWindow: Duration = .seconds(30),
    logger: MockLogger = MockLogger()
) -> TrUAPIWorkerManager {
    TrUAPIWorkerManager(
        builder: builder,
        collection: collection,
        references: { references },
        startupWindow: startupWindow,
        logger: logger
    )
}
