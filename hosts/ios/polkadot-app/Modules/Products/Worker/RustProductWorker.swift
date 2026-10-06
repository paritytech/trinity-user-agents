import Foundation
import Products
import TrUAPIHost

/// A declared background-only worker. The core owns its authority, signaling,
/// and Media lifetime; the worker receives only the generated product API.
actor RustProductWorker: ProductWorkerRunning {
    private let productUrl: URL
    private let execution: RustRuntimeEnvironment.ExecutionModel
    private let engineFactory: @Sendable () -> JSEngineProtocol
    private var engine: JSEngineProtocol?
    private var monitor: JSEngineMonitor?
    private var moduleBridge: JSESModuleBridge?
    private var started = false
    private var disposed = false

    init(productUrl: URL, execution: RustRuntimeEnvironment.ExecutionModel,
         engineFactory: @escaping @Sendable () -> JSEngineProtocol) {
        self.productUrl = productUrl
        self.execution = execution
        self.engineFactory = engineFactory
    }

    func start() async throws {
        guard !started, !disposed else { throw CancellationError() }
        started = true
        let engine = engineFactory()
        self.engine = engine
        do {
            await engine.registerJSDeviceCapabilityHandler { _ in .denied }
            let bootstrap = try execution.startBridge()
            let scriptsFactory = RustRuntimeScriptsFactory(bootstrapScript: bootstrap)
            try checkActive()
            try await engine.initialize(with: scriptsFactory.makeScripts())
            try checkActive()
            guard await engine.getState() == .ready else { throw ScriptExecutorError.engineInitFailed }
            await execution.media.watchEngine {
                switch await engine.getState() {
                case .destroyed, .error: return false
                default: return true
                }
            }
            try checkActive()
            let monitor = JSEngineMonitor(engine: engine,
                pauseEvent: .willResignActive, resumeEvent: .didBecomeActive)
            monitor.start()
            self.monitor = monitor
            let moduleBridge = JSESModuleBridge(engine: engine)
            self.moduleBridge = moduleBridge
            await moduleBridge.install()
            try checkActive()
            try await moduleBridge.executeScript(url: productUrl)
            try checkActive()
        } catch {
            // A boot may finish after disposal's first destroy; close that late
            // engine again rather than retaining a live, ownerless JS process.
            await dispose()
            await engine.destroy()
            throw error
        }
    }

    func dispose() async {
        guard !disposed else { return }
        disposed = true
        monitor?.stop(); monitor = nil
        await execution.media.close()
        let moduleBridge = moduleBridge; self.moduleBridge = nil
        await moduleBridge?.dispose()
        let engine = engine; self.engine = nil
        await engine?.destroy()
        execution.execution.stopWsBridge()
        execution.execution.close()
        execution.chainConnections.closeAll()
    }

    private func checkActive() throws {
        guard !disposed, !Task.isCancelled else { throw CancellationError() }
    }
}
