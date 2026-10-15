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
/// An uncaught error in the script ends the worker (06-OPEN-ITEMS O26). The
/// runtime reports it on ``faults``, and the manager then stops the worker, so
/// the core sees the worker go and a later request starts a new one. An
/// unhandled promise rejection does not end the worker: a browser and a Web
/// Worker keep running after one, and products often leave one by accident.
///
/// An actor so `start` and `dispose` never race. Actors are reentrant, so
/// `dispose` flips `disposed` before its first await and `start` re-checks it
/// after every one: a half-booted worker left behind would swallow every later
/// start while the core keeps counting the reference its holder took.
actor TrUAPIWorkerRuntime {
    /// The native function that the fault script calls with the message of an uncaught error.
    static let faultHandlerName = "__truapi_worker_fault__"

    /// Sends the message of each uncaught error of the page to ``faultHandlerName``. The script reads the
    /// handler when the error occurs, so it can go in before the engine registers the handler.
    static let faultScript = """
    (function() {
        window.addEventListener('error', function(event) {
            if (!(event instanceof ErrorEvent)) { return; }
            var handlers = window.webkit && window.webkit.messageHandlers;
            var handler = handlers && handlers.\(faultHandlerName);
            if (!handler) { return; }
            var where = event.filename ? ' at ' + event.filename + ':' + event.lineno : '';
            handler.postMessage(String(event.message) + where);
        });
    })();
    """

    /// The uncaught errors of the script, one message each. The stream ends when the runtime goes.
    nonisolated let faults: AsyncStream<String>
    private let faultContinuation: AsyncStream<String>.Continuation

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
        (faults, faultContinuation) = AsyncStream.makeStream(of: String.self)
    }

    nonisolated var execution: TrUAPIProductExecutionProtocol { executionModel.execution }

    func start() async throws {
        try checkNotDisposed()

        let bootstrap = try executionModel.startBridge()
        let scripts = try RustRuntimeScriptsFactory(bootstrapScript: bootstrap).makeScripts()
            + [JSEngineScript(content: Self.faultScript, insertionPoint: .atDocStart)]
        let jsEngine = try await bootEngine(scripts: scripts)
        try checkNotDisposed()

        // Before the product's module runs, so an error of its first line ends the worker too.
        let report: JSNativeHandler = { [faultContinuation, logger, productUrl] message in
            logger.error("[truapi] worker fault, the worker ends: \(message) (\(productUrl))")
            faultContinuation.yield(message)
        }
        await jsEngine.registerFunction(name: Self.faultHandlerName, handler: report)
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
        faultContinuation.finish()

        logger.debug("[truapi] worker stopped: \(productUrl)")
    }

    /// The capability handler is installed before the page exists, so a script
    /// that asks for a camera on its first line is prompted rather than denied.
    private func bootEngine(scripts: [JSEngineScript]) async throws -> JSEngineProtocol {
        let jsEngine = engineFactory()
        do {
            await jsEngine.registerJSDeviceCapabilityHandler(
                executionModel.osPermissionAsker.makeDeviceCapabilityHandler(
                    execution: executionModel.execution
                )
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
