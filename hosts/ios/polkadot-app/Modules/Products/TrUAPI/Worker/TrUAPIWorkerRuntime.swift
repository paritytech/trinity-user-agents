import Foundation
import Products
import TrUAPIHost

/// One product's worker, running headless behind a Worker execution.
///
/// The script runs in a web view with no screen of its own: the bootstrap
/// publishes the execution's loopback bridge before the page exists, the entry
/// module connects to it, and everything the worker draws comes back over the
/// execution rather than through any view. Every modality is served from this
/// one runtime, so a worker script is given the same engine whichever surface
/// asked for it.
///
/// An actor so `start` and `dispose` never race. Actors are reentrant, so
/// `dispose` flips `disposed` before its first await and `start` re-checks it
/// after every one: a half-booted worker left behind would swallow every later
/// start while the core keeps counting the reference its holder took.
actor TrUAPIWorkerRuntime {
    private let productUrl: URL
    private let executionModel: RustRuntimeEnvironment.ExecutionModel
    private let engineFactory: @Sendable () -> JSEngineProtocol
    private let logger: LoggerProtocol

    private var engine: JSEngineProtocol?
    private var engineMonitor: JSEngineMonitor?
    private var moduleBridge: JSESModuleBridge?
    private var disposed = false

    init(
        productUrl: URL,
        executionModel: RustRuntimeEnvironment.ExecutionModel,
        engineFactory: @escaping @Sendable () -> JSEngineProtocol,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productUrl = productUrl
        self.executionModel = executionModel
        self.engineFactory = engineFactory
        self.logger = logger
    }

    nonisolated var execution: TrUAPIProductExecutionProtocol { executionModel.execution }

    func start() async throws {
        try checkNotDisposed()

        let bootstrap = try executionModel.startBridge()
        let scripts = try RustRuntimeScriptsFactory(bootstrapScript: bootstrap).makeScripts()
        let jsEngine = try await bootEngine(scripts: scripts)
        try checkNotDisposed()

        let bridge = JSESModuleBridge(engine: jsEngine)
        await bridge.install()
        try checkNotDisposed()
        moduleBridge = bridge

        try await bridge.executeScript(url: productUrl)

        logger.debug("[truapi] worker running: \(productUrl)")
    }

    func dispose() async {
        guard !disposed else { return }
        disposed = true

        engineMonitor?.stop()
        engineMonitor = nil

        let moduleBridge = moduleBridge
        self.moduleBridge = nil
        await moduleBridge?.dispose()

        let engine = engine
        self.engine = nil
        await engine?.destroy()

        executionModel.execution.stopWsBridge()
        executionModel.execution.close()
        executionModel.chainConnections.closeAll()

        logger.debug("[truapi] worker stopped: \(productUrl)")
    }

    /// The capability handler is installed before the page exists, so a script
    /// that asks for a camera on its first line is prompted rather than denied.
    private func bootEngine(scripts: [JSEngineScript]) async throws -> JSEngineProtocol {
        let jsEngine = engineFactory()
        do {
            await jsEngine.registerJSDeviceCapabilityHandler(
                executionModel.osPermissionAsker.makeDeviceCapabilityHandler()
            )
            try checkNotDisposed()
            try await jsEngine.initialize(with: scripts)
            guard await jsEngine.getState() == .ready else { throw ScriptExecutorError.engineInitFailed }
            try checkNotDisposed()
        } catch {
            await jsEngine.destroy()
            throw error
        }

        let monitor = JSEngineMonitor(
            engine: jsEngine,
            pauseEvent: .willResignActive,
            resumeEvent: .didBecomeActive
        )
        monitor.start()
        engineMonitor = monitor

        engine = jsEngine
        return jsEngine
    }

    private func checkNotDisposed() throws {
        guard !disposed else { throw CancellationError() }
    }
}
