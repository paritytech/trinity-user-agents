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

        #expect(engine.initializedScripts.count == 3)
        #expect(engine.initializedScripts[0]
            .content == #"window.__truapi_localhost = { url: "ws://127.0.0.1:0/?t=test" };"#)
        #expect(engine.initializedScripts[0].insertionPoint == .atDocStart)
        #expect(engine.initializedScripts[1].content.contains("freezeAndDelete"))
        #expect(engine.initializedScripts[1].insertionPoint == .atDocStart)
        #expect(engine.initializedScripts[2].content == TrUAPIWorkerRuntime.faultScript)
        #expect(engine.initializedScripts[2].insertionPoint == .atDocStart)
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

    /// An uncaught error of the page reaches the manager through `faults` (06-OPEN-ITEMS O26). The handler is
    /// in place before the product's module runs.
    @Test
    func reportsAnUncaughtErrorOfThePage() async throws {
        let engine = MockJSEngine()
        let runtime = makeRuntime(engine: engine)

        try await runtime.start()
        try await engine.invokeNative(TrUAPIWorkerRuntime.faultHandlerName, args: "boom at worker.js:3")

        var faults = runtime.faults.makeAsyncIterator()
        #expect(await faults.next() == "boom at worker.js:3")

        await runtime.dispose()
        #expect(await faults.next() == nil)
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

/// A product worker compiled to wasm runs in the core, with no web view and no bridge.
struct TrUAPIWasmWorkerRuntimeTests {
    private let module = TrUAPIWorkerCode.wasmMagic + Data([0x01, 0x00, 0x00, 0x00])

    /// The core gets the module as it is, and nothing opens the loopback bridge, which a wasm worker cannot use.
    @Test
    func givesTheModuleToTheCoreWithNoBridge() async throws {
        let execution = MockProductExecution()
        let runtime = makeWasmRuntime(execution: execution)

        try await runtime.start()

        #expect(execution.startedWasmWorkers == [module])
        #expect(execution.startWsBridgeCallCount == 0)

        await runtime.dispose()
    }

    /// A trap or an error of the entry point ends the worker as an uncaught error of a script does (O26), so the
    /// manager stops it and the next request starts it again.
    @Test
    func reportsAFailureOfTheModuleAsAFault() async throws {
        let execution = MockProductExecution()
        let runtime = makeWasmRuntime(execution: execution)

        try await runtime.start()
        execution.endWasmWorker(.failed(reason: "worker trapped: out of fuel"))

        var faults = runtime.faults.makeAsyncIterator()
        #expect(await faults.next() == "worker trapped: out of fuel")

        await runtime.dispose()
    }

    /// An entry point that returns leaves nothing to serve the product, so the worker must not stay counted as
    /// running.
    @Test
    func reportsAReturnOfTheEntryPointAsAFault() async throws {
        let execution = MockProductExecution()
        let runtime = makeWasmRuntime(execution: execution)

        try await runtime.start()
        execution.endWasmWorker(.finished)

        var faults = runtime.faults.makeAsyncIterator()
        #expect(await faults.next() != nil)

        await runtime.dispose()
    }

    /// A stop of the manager is not a fault: the stop ends the module, and the fault stream ends with no message.
    @Test
    func stopsTheModuleOnDisposeWithNoFault() async throws {
        let execution = MockProductExecution()
        let runtime = makeWasmRuntime(execution: execution)

        try await runtime.start()
        await runtime.dispose()

        #expect(execution.stopWsBridgeCallCount == 1)
        #expect(execution.closeCallCount == 1)
        var faults = runtime.faults.makeAsyncIterator()
        #expect(await faults.next() == nil)
    }

    /// A stop that lands before the start must not leave a module running behind a closed execution.
    @Test
    func startsNoModuleAfterDispose() async throws {
        let execution = MockProductExecution()
        let runtime = makeWasmRuntime(execution: execution)

        await runtime.dispose()

        await #expect(throws: CancellationError.self) {
            try await runtime.start()
        }
        #expect(execution.startedWasmWorkers.isEmpty)
    }

    /// Only the wasm magic bytes select the core: every other entry, also an empty or a missing one, is a script.
    @Test
    func tellsAModuleFromAScriptByItsMagicBytes() {
        #expect(TrUAPIWorkerCode.wasmModule(entry: module, name: "a.dot/index.wasm") != nil)
        #expect(TrUAPIWorkerCode.wasmModule(entry: Data("export {}".utf8), name: "a.dot/index.js") == nil)
        #expect(TrUAPIWorkerCode.wasmModule(entry: Data([0x00, 0x61]), name: "a.dot/index.wasm") == nil)
        #expect(TrUAPIWorkerCode.wasmModule(entry: nil, name: "a.dot/index.wasm") == nil)
    }

    private func makeWasmRuntime(execution: MockProductExecution) -> TrUAPIWorkerRuntime {
        TrUAPIWorkerRuntime(
            code: .wasm(module, name: "verbosity.dot/ChatExtension/index.wasm"),
            executionModel: RustRuntimeEnvironment.ExecutionModel(
                execution: execution,
                chainConnections: MockChainConnections(),
                osPermissionAsker: OSPermissionAsker()
            )
        )
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
