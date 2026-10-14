import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// One headless worker runtime serves every modality, so a worker script is
/// given the same engine whichever surface asked for it.
struct TrUAPIWorkerRuntimeTests {
    /// The handler is what turns a `getUserMedia` call into the host's own
    /// permission prompt. Without it the engine denies the request outright, so
    /// the same worker script would be prompted under chat and refused here.
    @Test
    func letsTheWorkerAskForTheDeviceCapabilitiesItDeclares() async throws {
        let engine = MockJSEngine()
        let runtime = makeRuntime(engine: engine)

        try await runtime.start()

        #expect(engine.mediaHandlerWasInstalledAtInitialization)

        await runtime.dispose()
    }

    /// The bootstrap carries the loopback port and token, so it has to be in
    /// place before the page exists rather than evaluated into a live page.
    @Test
    func installsTheBootstrapBeforeTheProductLoads() async throws {
        let engine = MockJSEngine()
        let execution = MockProductExecution()
        let runtime = makeRuntime(engine: engine, execution: execution)

        try await runtime.start()

        #expect(execution.startWsBridgeCallCount == 1)
        #expect(execution.permissionRequests.isEmpty)

        #expect(engine.initializedScripts.count == 2)
        #expect(engine.initializedScripts[0]
            .content == #"window.__truapi_localhost = { url: "ws://127.0.0.1:0/?t=test" };"#)
        #expect(engine.initializedScripts[0].insertionPoint == .atDocStart)
        #expect(engine.initializedScripts[1].content.contains("freezeAndDelete"))
        #expect(engine.initializedScripts[1].insertionPoint == .atDocStart)
        #expect(!engine.evaluatedScripts.contains { $0.contains("__truapi_localhost") })
        #expect(!engine.evaluatedScripts.contains { $0.contains("freezeAndDelete") })

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

private func makeRuntime(
    engine: MockJSEngine,
    execution: MockProductExecution = MockProductExecution(),
    chainConnections: MockChainConnections = MockChainConnections()
) -> TrUAPIWorkerRuntime {
    TrUAPIWorkerRuntime(
        productUrl: URL(string: "https://product.invalid/worker.js")!,
        executionModel: RustRuntimeEnvironment.ExecutionModel(
            execution: execution,
            chainConnections: chainConnections,
            osPermissionAsker: OSPermissionAsker()
        ),
        engineFactory: { engine }
    )
}
