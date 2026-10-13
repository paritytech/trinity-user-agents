import Foundation
import os
import AsyncExtensions
import Products
import StructuredConcurrency
import TrUAPIHost

/// Runs product workers for as long as the core's reference ledger wants them.
///
/// A handler asks for the worker it needs with ``ensureWorker(for:)``, which
/// registers the request with the core and returns once the worker it decided
/// to run is up. The core counts those requests and reports only the
/// transitions across zero: a `.start` boots the product's worker behind its
/// one Worker execution, a `.stop` tears it down.
protocol TrUAPIWorkerManaging: AnyObject, Sendable {
    /// May arrive on any thread, including re-entrantly from inside a request,
    /// so the work is handed off rather than done here.
    func demandChanged(productId: ProductId, transition: WorkerTransition)

    /// Registers a request for `productId`'s worker and returns its execution
    /// once the core has let the worker start.
    ///
    /// The request is the caller's from the moment it asks, including when this
    /// throws, so it is given back with ``releaseWorker(for:)`` either way.
    /// A handler can be disposed while it is still waiting here, and a release
    /// that crossed the answer must not be the one that is lost.
    func ensureWorker(for productId: ProductId) async throws -> TrUAPIProductExecutionProtocol

    func releaseWorker(for productId: ProductId)

    /// What `productId`'s worker is built against, for the life of the session
    /// rather than of any one worker.
    func context(of productId: ProductId) -> ProductWorkerContext

    /// The product's worker execution while it runs, and nil while it does not.
    /// Followed by a handler that must survive the worker restarting under it.
    func executions(of productId: ProductId) -> AnyAsyncSequence<TrUAPIProductExecutionProtocol?>

    func currentExecution(of productId: ProductId) -> TrUAPIProductExecutionProtocol?

    /// Stops every worker this manager is running. Called when the session
    /// it was built for ends.
    func shutdown() async
}

/// Builds the pieces one product's worker needs: its archive, its execution and
/// the engine its script runs in.
protocol TrUAPIWorkerBuilding: Sendable {
    func makeRuntime(
        productId: ProductId,
        context: ProductWorkerContext,
        pocket: ProductPocketHostBridge
    ) async throws -> TrUAPIWorkerRuntime
}

actor TrUAPIWorkerManager: TrUAPIWorkerManaging {
    private struct Running {
        let boot: Boot
        let runtime: TrUAPIWorkerRuntime
        let pocket: ProductPocketHostBridge
    }

    /// What the manager holds for one product. A boot claims the product
    /// before its first await: actors are reentrant, so two starts that overlap
    /// would otherwise both pass the guard and build a worker, and a stop that
    /// landed mid-boot would find nothing to remove and leave the worker
    /// running with no way back to it.
    private enum Held {
        case booting(Boot)
        case running(Running)

        /// Whether this entry belongs to `boot`, so a boot only ever clears
        /// what it built. Android keys the same check on the worker itself.
        func belongsTo(_ boot: Boot) -> Bool {
            switch self {
            case let .booting(claimed): claimed === boot
            case let .running(worker): worker.boot === boot
            }
        }
    }

    /// One attempt to bring a product's worker up, as an identity. A stop and a
    /// restart can both land while an attempt is still in flight, and the one
    /// that finishes last must not clear what the others left.
    private final class Boot {}

    private let builder: any TrUAPIWorkerBuilding
    private let collection: any PocketCardStore
    /// The core's reference ledger, which is what decides whether a worker runs.
    private let references: @Sendable () throws -> any TrUAPIWorkerReferencing
    /// How long a request waits for the worker it asked for. A boot that fails
    /// publishes no execution, so without this a handler would wait for one
    /// forever.
    private let startupWindow: Duration
    private let logger: LoggerProtocol

    private var held: [ProductId: Held] = [:]
    private var draining: Task<Void, Never>?
    private var isShutDown = false

    /// Read from `context(of:)`, which a chat session calls before any worker
    /// exists, so it is held beside the actor's own state rather than in it.
    private let openContexts = OSAllocatedUnfairLock<[ProductId: ProductWorkerContext]>(initialState: [:])

    /// Transitions are booked one at a time, in the order the ledger sent them.
    /// Handing each to its own task leaves the order to the scheduler, where a
    /// start and the stop that cancels it can overtake each other.
    private let transitions = AsyncStream<(ProductId, WorkerTransition)>.makeStream()
    private let executionSubject = AsyncCurrentValueSubject<[ProductId: TrUAPIProductExecutionProtocol]>([:])

    init(
        builder: any TrUAPIWorkerBuilding,
        collection: any PocketCardStore,
        references: @escaping @Sendable () throws -> any TrUAPIWorkerReferencing,
        startupWindow: Duration = .seconds(30),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.builder = builder
        self.collection = collection
        self.references = references
        self.startupWindow = startupWindow
        self.logger = logger

        Task { await self.beginFollowing() }
    }

    nonisolated func ensureWorker(for productId: ProductId) async throws -> TrUAPIProductExecutionProtocol {
        try references().acquireWorker(productId: productId)

        return try await withTimeout(startupWindow) { [self] in
            for try await execution in executions(of: productId) {
                if let execution { return execution }
            }

            throw TrUAPIWorkerError.noWorker(productId)
        }
    }

    nonisolated func releaseWorker(for productId: ProductId) {
        do {
            try references().releaseWorker(productId: productId)
        } catch {
            logger.error("[truapi] \(productId)'s worker request could not be given back: \(error)")
        }
    }

    /// Started from a task rather than in `init`, because the drain task
    /// captures the manager and an actor's initializer cannot reach its own
    /// storage once it has escaped. A shutdown can therefore land first, and
    /// must not be followed by a drain loop nothing holds a way back to.
    private func beginFollowing() {
        guard !isShutDown else { return }

        draining = Task { await self.drainTransitions() }
    }

    private func drainTransitions() async {
        for await (productId, transition) in transitions.stream {
            await apply(transition, to: productId)
        }
    }

    /// The workers outlive every screen, so nothing else ever tears them down:
    /// a manager dropped without this leaves a headless web view and its
    /// chain connections running for the rest of the process.
    func shutdown() async {
        isShutDown = true
        transitions.continuation.finish()
        draining?.cancel()
        draining = nil

        // Over a snapshot: `stop` takes each product out of the map it would
        // otherwise be iterating, and awaits inside it let more land.
        for productId in Array(held.keys) {
            await stop(productId)
        }
    }

    nonisolated func demandChanged(productId: ProductId, transition: WorkerTransition) {
        transitions.continuation.yield((productId, transition))
    }

    nonisolated func context(of productId: ProductId) -> ProductWorkerContext {
        openContexts.withLock { open in
            if let context = open[productId] { return context }

            let context = ProductWorkerContext()
            open[productId] = context
            return context
        }
    }

    nonisolated func executions(of productId: ProductId) -> AnyAsyncSequence<TrUAPIProductExecutionProtocol?> {
        executionSubject
            .map { $0[productId] }
            .eraseToAnyAsyncSequence()
    }

    nonisolated func currentExecution(of productId: ProductId) -> TrUAPIProductExecutionProtocol? {
        executionSubject.value[productId]
    }

    private func apply(_ transition: WorkerTransition, to productId: ProductId) async {
        switch transition {
        case .start: start(productId)
        case .stop: await stop(productId)
        }
    }

    /// Claims the product and returns, leaving the slow half of the boot to its
    /// own task. Fetching an archive and booting a web view can stall for as
    /// long as the network does, and held against this queue that would keep
    /// every other product off its worker.
    private func start(_ productId: ProductId) {
        guard held[productId] == nil else { return }
        let boot = Boot()
        held[productId] = .booting(boot)

        Task { await self.bringUp(productId, as: boot) }
    }

    private func bringUp(_ productId: ProductId, as boot: Boot) async {
        let bridge = ProductPocketHostBridge(productId: productId, collection: collection)

        do {
            let runtime = try await builder.makeRuntime(
                productId: productId,
                context: context(of: productId),
                pocket: bridge
            )
            guard held[productId]?.belongsTo(boot) == true else {
                await runtime.dispose()
                return
            }
            held[productId] = .running(Running(boot: boot, runtime: runtime, pocket: bridge))

            // The worker's card list is served from the bridge's snapshot, and
            // the script that subscribes to it comes up inside `start()`.
            await bridge.begin()
            try await runtime.start()

            guard case let .running(worker)? = held[productId], worker.boot === boot else { return }

            // Published before the bridge republishes, because republishing
            // reads the execution back out of this map.
            executionSubject.send(executionSubject.value.merging([productId: runtime.execution]) { _, new in new })
            bridge.start { [weak self] cards in
                self?.publishCards(cards, of: productId)
            }
        } catch {
            // Only a failure of the boot still holding the product: a stop may
            // already have cleared this one and a restart taken its place, and
            // clearing that would swallow a start the core never repeats.
            guard held[productId]?.belongsTo(boot) == true else { return }

            logger.error("[truapi] \(productId)'s worker failed to start: \(error)")
            // A failed boot left in the map would swallow every later start,
            // while the core keeps counting the reference the holder took and
            // so never sends another stop to clear it.
            await stop(productId)
        }
    }

    private func stop(_ productId: ProductId) async {
        // A boot in flight is cleared here and disposes what it built when it
        // finds its claim gone, so there is nothing else to tear down yet.
        guard case let .running(worker)? = held.removeValue(forKey: productId) else { return }

        worker.pocket.stop()
        var executions = executionSubject.value
        executions[productId] = nil
        executionSubject.send(executions)

        await worker.runtime.dispose()
    }

    private nonisolated func publishCards(_ cards: [PocketCard], of productId: ProductId) {
        executionSubject.value[productId]?.notifyPocketCardsChanged(cards: cards)
    }
}
