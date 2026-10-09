import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// One headless worker runtime serves every modality, so a worker script is
/// given the same engine whichever surface asked for it.
@MainActor
struct TrUAPIWorkerRuntimeTests {
    /// Guest capture stays denied even when the host-managed Media backend exists.
    @Test
    func deniesDirectProductCapture() async throws {
        let engine = MockJSEngine()
        let execution = MockProductExecution()
        let runtime = makeRuntime(engine: engine, execution: execution)

        try await runtime.start()

        #expect(engine.mediaHandlerWasInstalledAtInitialization)
        #expect(execution.permissionRequests.isEmpty)
        let capture = try #require(engine.deviceCapabilityHandler)
        #expect(try await capture(.camera) == .denied)
        #expect(try await capture(.microphone) == .denied)

        await runtime.dispose()
    }


    /// A stop landing mid-boot must not leave the product's script running: the
    /// engine goes before any product code is evaluated.
    @Test
    func destroysTheEngineBeforeProductCodeWhenDisposedMidBoot() async throws {
        let engine = MockJSEngine()
        let execution = MockProductExecution()
        let runtime = makeRuntime(engine: engine, execution: execution)
        engine.onInitialize = { await runtime.dispose() }

        await #expect(throws: CancellationError.self) {
            try await runtime.start()
        }

        #expect(engine.destroyCallCount == 1)
        #expect(engine.evaluatedScripts.isEmpty)
        #expect(execution.closeCallCount == 1)
        #expect(execution.stopWsBridgeCallCount == 1)
    }

    @Test
    func refusesProductCodeWhenItsExecutionIsRevokedDuringBoot() async throws {
        let engine = MockJSEngine()
        let execution = MockProductExecution()
        let runtime = makeRuntime(engine: engine, execution: execution)
        engine.onInitialize = { execution.close() }

        await #expect(throws: CancellationError.self) {
            try await runtime.start()
        }

        #expect(engine.destroyCallCount == 1)
        #expect(engine.evaluatedScripts.isEmpty)
        await runtime.dispose()
    }

    /// The worker owns the execution, so stopping it is what closes the
    /// execution, its bridge and its chain pool, exactly once.
    @Test
    func tearsDownTheExecutionOnce() async throws {
        let execution = MockProductExecution()
        let chainConnections = MockChainConnections()
        let runtime = makeRuntime(
            engine: MockJSEngine(),
            execution: execution,
            chainConnections: chainConnections
        )

        try await runtime.start()
        await runtime.dispose()
        await runtime.dispose()

        #expect(execution.stopWsBridgeCallCount == 1)
        #expect(execution.closeCallCount == 1)
        #expect(chainConnections.closeAllCallCount == 1)
    }
}

@MainActor
private func makeRuntime(
    engine: MockJSEngine,
    execution: MockProductExecution = MockProductExecution(),
    chainConnections: MockChainConnections = MockChainConnections()
) -> TrUAPIWorkerRuntime {
    TrUAPIWorkerRuntime(
        productUrl: URL(string: "https://product.invalid/worker.js")!,
        executionModel: makeExecutionModel(execution: execution, chainConnections: chainConnections),
        engineFactory: { engine }
    )
}
