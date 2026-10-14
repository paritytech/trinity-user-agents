import Foundation
import Products
import TrUAPIHost
import UIKitExt

/// Rust SPA runtime: resolves the product content, publishes the scheme
/// handler, injects the bootstrap + container scripts into the engine, and
/// returns the page URL. The TrUAPI core serves product requests over its
/// localhost ws-bridge; no ContainerBridge is installed in rust mode. Camera/mic
/// media capture (getUserMedia) is answered by a `JSDeviceCapabilityHandler`
/// registered on the engine.
///
/// An actor so `start`/`dispose` never race on runtime state. Actors are
/// reentrant, so `dispose()` can interleave while `start` is suspended:
/// `dispose` flips `disposed` before its first await and `start` re-checks
/// it after every await, destroying anything it created in the gap.
actor SPARustRuntime {
    private let executionModel: RustRuntimeEnvironment.ExecutionModel
    private let configuration: SPAConfiguration
    private let dotNsResolver: DotNsResolverProtocol
    private let productResolver: ProductResolving
    private let schemeHandlerProxy: SchemeHandlerProxy
    private let logger: LoggerProtocol

    private var engine: JSEngineProtocol?
    private var engineMonitor: JSEngineMonitor?
    private var started = false
    private var disposed = false

    init(
        executionModel: RustRuntimeEnvironment.ExecutionModel,
        configuration: SPAConfiguration,
        dotNsResolver: DotNsResolverProtocol,
        productResolver: ProductResolving,
        schemeHandlerProxy: SchemeHandlerProxy,
        logger: LoggerProtocol
    ) {
        self.executionModel = executionModel
        self.configuration = configuration
        self.dotNsResolver = dotNsResolver
        self.productResolver = productResolver
        self.schemeHandlerProxy = schemeHandlerProxy
        self.logger = logger
    }

    deinit {
        // Locals first: assert's and &&'s autoclosures are nonisolated;
        // direct reads of the (Sendable) stored properties are only legal
        // in the deinit body itself.
        let started = started
        let disposed = disposed
        assert(!started || disposed, "SPARustRuntime dropped without dispose()")
    }
}

extension SPARustRuntime: SPARuntimeProtocol {
    func start(with engine: JSEngineProtocol) async throws -> URL {
        // Single-shot: a second start would re-activate the session with no
        // matching teardown; a post-dispose start must not run at all.
        guard !started, !disposed else { throw CancellationError() }
        started = true

        let initialURL: URL =
            switch configuration.contentSource {
            case .dotNs:
                try await prepareDotNsContent()
            case let .directURL(url):
                url
            }

        try checkNotDisposed()
        let bootstrapScript = try executionModel.startBridge()
        let scriptsFactory = RustRuntimeScriptsFactory(bootstrapScript: bootstrapScript)

        await engine.registerJSDeviceCapabilityHandler(
            executionModel.osPermissionAsker.makeDeviceCapabilityHandler()
        )

        try await engine.initialize(with: scriptsFactory.makeScripts() + [.disableZoom])

        // Disposed while initializing: dispose captured nil for the engine,
        // so this start is the only owner left — destroy before bailing.
        guard !disposed else {
            await engine.destroy()
            throw CancellationError()
        }

        let monitor = JSEngineMonitor(engine: engine)
        monitor.start()
        engineMonitor = monitor
        self.engine = engine

        logger.debug("SPA(rust): runtime started for \(configuration.page.host.name)")

        return initialURL
    }

    func dispose() async {
        guard !disposed else { return }
        // Flipped before the first suspension: any start resuming after this
        // point observes it and unwinds.
        disposed = true

        engineMonitor?.stop()
        engineMonitor = nil

        let engine = engine
        self.engine = nil
        await engine?.destroy()

        executionModel.execution.stopWsBridge()
        executionModel.execution.close()
        executionModel.chainConnections.closeAll()

        logger.debug("SPA(rust): runtime disposed for \(configuration.page.host.name)")
    }
}

private extension SPARustRuntime {
    func prepareDotNsContent() async throws -> URL {
        let domain = configuration.page.host.toDotDomain()

        // Bytes come from the executable's own subname; the origin stays the base domain, which
        // is what permission grants and web storage are keyed by.
        let contentId = try await productResolver.resolve(domain).contentId(for: configuration.executable)
        let contentURL = try await dotNsResolver.resolveToLocalURL(dotNsName: contentId)

        let schemeHandler = ProductScriptSchemeHandler(
            productId: domain,
            entryRelativePath: ProductBundle.indexHTML,
            productFileProvider: DotNsFileProvider(contentURL: contentURL)
        )

        guard let productURL = schemeHandler.getProductUrl() else {
            throw ScriptExecutorError.scriptNotFound(productId: domain)
        }

        logger.debug("SPA(rust): '\(domain)' content resolved to \(contentURL.path)")

        // Don't publish a handler to the shared proxy after dispose ran.
        try checkNotDisposed()
        await schemeHandlerProxy.setHandler(schemeHandler)

        return configuration.page.applied(to: productURL)
    }

    func checkNotDisposed() throws {
        guard !disposed else { throw CancellationError() }
    }
}
