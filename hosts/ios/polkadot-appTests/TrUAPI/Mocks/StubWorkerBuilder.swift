import Foundation
import os
import Products
import StructuredConcurrencyTestSupport
import TrUAPIHost
@testable import polkadot_app

/// Builds workers whose execution is a `MockProductExecution`, with the
/// failures, delays and stalls a boot can hit placed by the test.
final class StubWorkerBuilder: TrUAPIWorkerBuilding, @unchecked Sendable {
    /// Boots run on concurrent tasks while the test reads and flips these, so
    /// every field sits behind the one lock.
    private struct State {
        var built: [ProductId] = []
        var executions: [ProductId: MockProductExecution] = [:]
        var cardsWhenTheEngineBooted: [PocketCard] = []
        var bridge: ProductPocketHostBridge?
        var cannotBuild: Bool
        var engineFails: Bool
        var buildCount = 0
    }

    private let state: OSAllocatedUnfairLock<State>
    var built: [ProductId] { state.withLock { $0.built } }
    /// The executions built so far, by product, so a test can read what the core
    /// was told through them.
    var executions: [ProductId: MockProductExecution] { state.withLock { $0.executions } }
    /// What the bridge would have answered the moment the worker's engine came
    /// up, which is when its script subscribes to the card list.
    private(set) var cardsWhenTheEngineBooted: [PocketCard] {
        get { state.withLock { $0.cardsWhenTheEngineBooted } }
        set { state.withLock { $0.cardsWhenTheEngineBooted = newValue } }
    }

    /// The bridge of the worker built last, so a test can read the slice the
    /// core is being served after the collection moves.
    var bridge: ProductPocketHostBridge? { state.withLock { $0.bridge } }
    /// Advances only when a test resumes a sleep, so the pauses inside a boot
    /// are placed by the test rather than by the wall clock.
    let clock = ManualClock()

    var cannotBuild: Bool {
        get { state.withLock { $0.cannotBuild } }
        set { state.withLock { $0.cannotBuild = newValue } }
    }

    var engineFails: Bool {
        get { state.withLock { $0.engineFails } }
        set { state.withLock { $0.engineFails = newValue } }
    }

    private let buildDelay: Duration
    /// The only product whose build waits on `buildDelay`, so a barrier product never waits.
    private let slowProduct: ProductId = "game.paseo"
    /// How long the first worker sits inside `start()` before failing. The
    /// window a stop and a restart have to land in.
    private let firstStartFailsAfter: Duration?
    /// The product whose build hangs until `releaseTheStalledBoot()`, standing
    /// in for an archive fetch that never comes back.
    private let stalledProduct: ProductId?
    private let stall = AsyncStream<Void>.makeStream()

    init(
        cannotBuild: Bool = false,
        engineFails: Bool = false,
        buildDelay: Duration = .zero,
        firstStartFailsAfter: Duration? = nil,
        stalledProduct: ProductId? = nil
    ) {
        state = OSAllocatedUnfairLock(initialState: State(cannotBuild: cannotBuild, engineFails: engineFails))
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

        let execution = MockProductExecution()
        state.withLock {
            $0.bridge = pocket
            $0.built.append(productId)
            $0.executions[productId] = execution
        }

        if productId == slowProduct, buildDelay > .zero { try await clock.sleep(for: buildDelay) }
        if productId == stalledProduct {
            for await _ in stall.stream {}
        }

        let isFirstBuild = state.withLock {
            $0.buildCount += 1
            return $0.buildCount == 1
        }
        let firstStartFailsAfter = firstStartFailsAfter
        let engineFails = engineFails
        let clock = clock
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
                    return SlowFailingJSEngine(after: firstStartFailsAfter, clock: clock)
                }
                return MockJSEngine()
            }
        )
    }
}
