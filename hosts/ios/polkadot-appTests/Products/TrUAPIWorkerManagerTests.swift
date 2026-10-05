import Foundation
import os
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The core counts worker references and reports only the transitions across
/// zero, so the manager must be exact: a worker left running burns a web
/// view, and one left half-booted swallows every later start.
struct TrUAPIWorkerManagerTests {
    @Test
    func startsTheWorkerOnTheFirstDemand() async throws {
        let builder = StubBuilder()
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await settle()

        #expect(builder.built == ["game.paseo"])
        #expect(manager.currentExecution(of: "game.paseo") != nil)
    }

    /// The core reports only transitions across zero, but a stop and a start can
    /// overtake each other; a second start while one is running must not open a
    /// second execution for the same product.
    @Test
    func doesNotStartASecondWorkerForTheSameProduct() async throws {
        let builder = StubBuilder()
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await settle()
        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await settle()

        #expect(builder.built == ["game.paseo"])
    }

    @Test
    func stopsTheWorkerAndForgetsItsExecution() async throws {
        let builder = StubBuilder()
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await settle()
        manager.demandChanged(productId: "game.paseo", transition: .stop)
        try await settle()

        #expect(manager.currentExecution(of: "game.paseo") == nil)
    }

    /// A worker whose engine fails once it is already in the running map must
    /// leave nothing behind: the core keeps counting the reference the card
    /// holds, so no further stop arrives to clear a half-started entry, and
    /// every later start would be swallowed by the entry that is still there.
    @Test
    func leavesNothingBehindWhenTheEngineFailsToBoot() async throws {
        let builder = StubBuilder(engineFails: true)
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await settle()

        #expect(manager.currentExecution(of: "game.paseo") == nil)

        builder.engineFails = false
        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await settle()

        #expect(manager.currentExecution(of: "game.paseo") != nil)
    }

    /// A product with no worker cannot be built at all, which must be as clean
    /// a failure as one that breaks while booting.
    @Test
    func leavesNothingBehindWhenTheWorkerCannotBeBuilt() async throws {
        let builder = StubBuilder(cannotBuild: true)
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await settle()

        #expect(manager.currentExecution(of: "game.paseo") == nil)

        builder.cannotBuild = false
        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await settle()

        #expect(manager.currentExecution(of: "game.paseo") != nil)
    }

    /// A stop for a product that never started is what the core sends when a
    /// card's worker was refused, and it must not tear anything else down.
    @Test
    func ignoresAStopForAWorkerThatNeverStarted() async throws {
        let builder = StubBuilder()
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await settle()
        manager.demandChanged(productId: "other.paseo", transition: .stop)
        try await settle()

        #expect(manager.currentExecution(of: "game.paseo") != nil)
    }

    /// The worker's card list is served from the bridge's snapshot, and the
    /// script that subscribes to it comes up inside `runtime.start()`. A
    /// snapshot filled after that point leaves the product reading an empty
    /// Pocket for the whole life of its worker.
    @Test
    func fillsTheCardListBeforeTheWorkersScriptComesUp() async throws {
        let builder = StubBuilder()
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await settle()

        #expect(builder.cardsWhenTheEngineBooted.map(\.cardId) == ["loyalty"])
    }

    /// Republishing reads the execution back out of the published map, so a
    /// republish that ran before the execution was published reached nobody,
    /// and the bridge, whose snapshot had already moved, never sent another.
    @Test
    func tellsTheCoreWhatTheWorkerHoldsOnceItsExecutionIsPublished() async throws {
        let builder = StubBuilder()
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await settle()

        #expect(builder.execution?.pocketCardNotifications.last?.map(\.cardId) == ["loyalty"])
    }

    /// Building a worker fetches its archive, which is exactly when two cards
    /// for one product scroll into view together. Actors are reentrant, so both
    /// starts reach the guard while the first is still waiting on it: two
    /// workers for one product is two headless web views, and only the last is
    /// ever stopped.
    @Test
    func doesNotStartASecondWorkerWhenTwoDemandsOverlap() async throws {
        let builder = StubBuilder(buildDelay: .milliseconds(30))
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await settle()
        try await Task.sleep(for: .milliseconds(200))

        #expect(builder.built == ["game.paseo"])
    }

    /// A stop that lands while the worker is still being built finds nothing to
    /// remove, and the boot then publishes an execution no stop can reach: the
    /// web view and its chain connections run for the rest of the session.
    @Test
    func disposesAWorkerThatWasStoppedWhileItWasStillBooting() async throws {
        let builder = StubBuilder(buildDelay: .milliseconds(80))
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await Task.sleep(for: .milliseconds(20))
        manager.demandChanged(productId: "game.paseo", transition: .stop)
        try await settle()
        try await Task.sleep(for: .milliseconds(300))

        #expect(manager.currentExecution(of: "game.paseo") == nil)
        #expect(builder.execution?.closeCallCount == 1)
    }

    /// The ledger delivers its transitions in order, so the manager has to
    /// apply them in order. A start, a stop and a start leave the worker
    /// running; applied out of order the pair of starts collapses into one and
    /// the stop lands last, leaving the card with no worker at all.
    @Test
    func appliesABurstOfTransitionsInTheOrderTheyArrive() async throws {
        let builder = StubBuilder(buildDelay: .milliseconds(40))
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        manager.demandChanged(productId: "game.paseo", transition: .stop)
        manager.demandChanged(productId: "game.paseo", transition: .start)

        try await settle()
        try await Task.sleep(for: .milliseconds(300))

        #expect(manager.currentExecution(of: "game.paseo") != nil)
    }

    /// A boot that fails after a stop has already cleared it must not take the
    /// boot that replaced it down too. The core counts the card's reference and
    /// reports only transitions across zero, so a start swallowed here never
    /// comes again and the card is left on whatever it last drew.
    @Test
    func aFailedBootDoesNotClearTheBootThatReplacedIt() async throws {
        let builder = StubBuilder(firstStartFailsAfter: .milliseconds(300))
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await Task.sleep(for: .milliseconds(50))

        manager.demandChanged(productId: "game.paseo", transition: .stop)
        try await Task.sleep(for: .milliseconds(20))
        manager.demandChanged(productId: "game.paseo", transition: .start)

        try await settle()
        try await Task.sleep(for: .milliseconds(400))

        #expect(manager.currentExecution(of: "game.paseo") != nil)
    }

    /// Every product's transitions arrive down one queue, and building a worker
    /// fetches its archive over the network. A product whose archive never
    /// comes back must not leave every other product's card without a worker
    /// for as long as the fetch hangs.
    @Test
    func aStalledBootDoesNotHoldUpAnotherProduct() async throws {
        let builder = StubBuilder(stalledProduct: "stalled.paseo")
        let manager = makeManager(builder: builder, collection: collectionHolding([loyalty]))

        manager.demandChanged(productId: "stalled.paseo", transition: .start)
        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await settle()

        #expect(manager.currentExecution(of: "game.paseo") != nil)
        #expect(manager.currentExecution(of: "stalled.paseo") == nil)

        builder.releaseTheStalledBoot()
    }

    /// A worker already running is told about a card added later because its
    /// bridge follows storage. Nothing announces the write, so a writer that
    /// forgot to say it wrote cannot leave a product's own list stale.
    @Test
    func tellsARunningWorkerAboutACardAddedLater() async throws {
        let builder = StubBuilder()
        let collection = InMemoryPocketCardStore([loyalty])
        let manager = makeManager(builder: builder, collection: collection)

        manager.demandChanged(productId: "game.paseo", transition: .start)
        try await settle()
        #expect(try builder.bridge?.listCards().map(\.cardId) == ["loyalty"])

        try await collection.add(streak, face: .nil)
        try await settle()

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

/// The manager hands its work to a task, so the assertions wait for it
/// rather than for a fixed time.
private func settle() async throws {
    for _ in 0 ..< 20 {
        await Task.yield()
    }
    try await Task.sleep(for: .milliseconds(20))
}

private func collectionHolding(_ cards: [PocketCardEntry]) -> any PocketCardStore {
    InMemoryPocketCardStore(cards)
}

private func makeManager(
    builder: any TrUAPIWorkerBuilding,
    collection: any PocketCardStore,
    references: StubWorkerReferences = StubWorkerReferences(),
    startupWindow: Duration = .seconds(30)
) -> TrUAPIWorkerManager {
    TrUAPIWorkerManager(
        builder: builder,
        collection: collection,
        references: { references },
        startupWindow: startupWindow
    )
}

private final class StubBuilder: TrUAPIWorkerBuilding, @unchecked Sendable {
    private(set) var built: [ProductId] = []
    /// The execution of the worker built last, so a test can read what the core
    /// was told through it.
    private(set) var execution: MockProductExecution?
    /// What the bridge would have answered the moment the worker's engine came
    /// up, which is when its script subscribes to the card list.
    private(set) var cardsWhenTheEngineBooted: [PocketCard] = []
    /// The bridge of the worker built last, so a test can read the slice the
    /// core is being served after the collection moves.
    private(set) var bridge: ProductPocketHostBridge?

    var cannotBuild: Bool
    var engineFails: Bool
    private let buildDelay: Duration
    /// How long the first worker sits inside `start()` before failing. The
    /// window a stop and a restart have to land in.
    private let firstStartFailsAfter: Duration?
    /// The product whose build hangs until `releaseTheStalledBoot()`, standing
    /// in for an archive fetch that never comes back.
    private let stalledProduct: ProductId?
    private let stall = AsyncStream<Void>.makeStream()
    private var buildCount = 0

    init(
        cannotBuild: Bool = false,
        engineFails: Bool = false,
        buildDelay: Duration = .zero,
        firstStartFailsAfter: Duration? = nil,
        stalledProduct: ProductId? = nil
    ) {
        self.cannotBuild = cannotBuild
        self.engineFails = engineFails
        self.buildDelay = buildDelay
        self.firstStartFailsAfter = firstStartFailsAfter
        self.stalledProduct = stalledProduct
    }

    /// Lets the stalled build finish, so the test leaves no task parked on it.
    func releaseTheStalledBoot() {
        stall.continuation.finish()
    }

    func makeRuntime(
        productId: ProductId,
        context _: ProductWorkerContext,
        pocket: ProductPocketHostBridge
    ) async throws -> TrUAPIWorkerRuntime {
        if cannotBuild { throw TrUAPIWorkerError.noWorker(productId) }
        if buildDelay > .zero { try await Task.sleep(for: buildDelay) }
        if productId == stalledProduct {
            for await _ in stall.stream {}
        }

        bridge = pocket

        built.append(productId)
        let execution = MockProductExecution()
        self.execution = execution

        buildCount += 1
        let isFirstBuild = buildCount == 1
        let firstStartFailsAfter = firstStartFailsAfter
        let engineFails = engineFails
        return TrUAPIWorkerRuntime(
            productUrl: URL(string: "https://product.invalid/worker.js")!,
            executionModel: RustRuntimeEnvironment.ExecutionModel(
                execution: execution,
                chainConnections: MockChainConnections(),
                osPermissionAsker: OSPermissionAsker()
            ),
            engineFactory: { [weak self] in
                self?.cardsWhenTheEngineBooted = (try? pocket.listCards()) ?? []
                if engineFails { return FailingJSEngine() }
                if isFirstBuild, let firstStartFailsAfter {
                    return SlowFailingJSEngine(after: firstStartFailsAfter)
                }
                return MockJSEngine()
            }
        )
    }
}

/// An engine that hangs and then fails, which is what a worker whose page is
/// still loading when its execution is closed underneath it looks like.
private final class SlowFailingJSEngine: JSEngineProtocol, @unchecked Sendable {
    private let delay: Duration

    init(after delay: Duration) {
        self.delay = delay
    }

    func getState() async -> JSEngineState { .error("no page") }
    func initialize(with _: [JSEngineScript]) async throws {
        try? await Task.sleep(for: delay)
        throw ScriptExecutorError.engineInitFailed
    }

    func evaluate(_: String) async throws -> Any? { nil }
    func registerFunction(name _: String, handler _: @escaping JSNativeHandler) async {}
    func dispatchEvent(actionId _: String, payload _: String) async throws {}
    func destroy() async {}
    func registerJSDeviceCapabilityHandler(_: @escaping JSDeviceCapabilityHandler) async {}
}

/// An engine whose page never comes up, which is what a worker archive that
/// cannot be loaded looks like from here.
private final class FailingJSEngine: JSEngineProtocol, @unchecked Sendable {
    func getState() async -> JSEngineState { .error("no page") }
    func initialize(with _: [JSEngineScript]) async throws { throw ScriptExecutorError.engineInitFailed }
    func evaluate(_: String) async throws -> Any? { nil }
    func registerFunction(name _: String, handler _: @escaping JSNativeHandler) async {}
    func dispatchEvent(actionId _: String, payload _: String) async throws {}
    func destroy() async {}
    func registerJSDeviceCapabilityHandler(_: @escaping JSDeviceCapabilityHandler) async {}
}
