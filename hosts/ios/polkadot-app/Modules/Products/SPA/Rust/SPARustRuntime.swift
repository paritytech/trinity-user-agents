import Foundation
import IssueMonitoring
import Products
import TrUAPIHost
import UIKitExt

/// Rust SPA runtime: opens the product content its strategy names, publishes the scheme
/// handler, injects the bootstrap + container scripts into the engine, and
/// returns the page URL. The TrUAPI core serves product requests over its
/// localhost ws-bridge; no ContainerBridge is installed in rust mode. Camera/mic
/// media capture (getUserMedia) and WebKit's motion request are answered by a
/// `JSDeviceCapabilityHandler` registered on the engine.
///
/// An actor so `start`/`dispose` never race on runtime state. Actors are
/// reentrant, so `dispose()` can interleave while `start` is suspended:
/// `dispose` flips `disposed` before its first await and `start` re-checks
/// it after every await, destroying anything it created in the gap.
actor SPARustRuntime {
    struct ProductOpening {
        let loader: ProductContentLoading
        let strategy: ExecutableKindStrategy
        let resolver: ProductResolving
    }

    private let executionModel: RustRuntimeEnvironment.ExecutionModel
    private let configuration: SPAConfiguration
    private let productOpening: ProductOpening
    private let schemeHandlerProxy: SchemeHandlerProxy
    private let logger: LoggerProtocol
    private let hostEventRecorder: HostEventRecording
    private let hostSessionId = UUID()

    private var engine: JSEngineProtocol?
    private var engineMonitor: JSEngineMonitor?
    private var started = false
    private var disposed = false
    private var servedContent: ProductContent?

    init(
        executionModel: RustRuntimeEnvironment.ExecutionModel,
        configuration: SPAConfiguration,
        productOpening: ProductOpening,
        schemeHandlerProxy: SchemeHandlerProxy,
        logger: LoggerProtocol,
        hostEventRecorder: HostEventRecording
    ) {
        self.executionModel = executionModel
        self.configuration = configuration
        self.productOpening = productOpening
        self.schemeHandlerProxy = schemeHandlerProxy
        self.logger = logger
        self.hostEventRecorder = hostEventRecorder
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
    func start(
        with engine: JSEngineProtocol,
        onPhase: @escaping ProductLoadPhaseHandler
    ) async throws -> SPARuntimeStart {
        // Single-shot: a second start would re-activate the session with no
        // matching teardown; a post-dispose start must not run at all.
        guard !started, !disposed else { throw CancellationError() }
        started = true

        let initialURL: URL =
            switch configuration.contentSource {
            case .dotNs:
                try await serveDotNsContent(onPhase: onPhase)
            case let .directURL(url):
                url
            case let .bundled(directory):
                try await configuration.page.applied(to: serveFiles(at: directory))
            #if DEBUG
                case let .devServer(url, _):
                    url
            #endif
            }

        let resolution = await productResolution()

        try checkNotDisposed()
        let bootstrapScript = try executionModel.startBridge()
        let scriptsFactory = RustRuntimeScriptsFactory(
            bootstrapScript: bootstrapScript,
            hostTelemetry: IssueMonitoringFactory.createNativeHostTelemetry(
                hostSessionID: hostSessionId,
                truapiHostVersion: truapiHostVersion()
            )
        )

        await engine.registerJSDeviceCapabilityHandler(
            executionModel.osPermissionAsker.makeDeviceCapabilityHandler(
                execution: executionModel.execution
            )
        )

        try await engine.initialize(with: scriptsFactory.makeScripts() + [.disableZoom])

        hostEventRecorder.record(.webViewCreated(
            hostSessionID: hostSessionId,
            origin: .current,
            truapiHostVersion: truapiHostVersion(),
            monotonicMilliseconds: HostEventClock.monotonicMilliseconds()
        ))

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

        return SPARuntimeStart(url: initialURL, resolution: resolution)
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

    func useLatestBuild() async throws -> ProductContent? {
        try checkNotDisposed()

        guard let servedContent else { return nil }

        let latest = try await productOpening.loader.open(
            configuration.productDomain,
            strategy: .fixed(servedContent.kind),
            policy: .untilCancelled
        )
        try checkNotDisposed()

        guard latest.archive != servedContent.archive else { return servedContent }

        _ = try await serve(latest)
        return latest
    }
}

private extension SPARustRuntime {
    /// Bytes come from the executable's own subname; the origin stays the base domain, which is
    /// what permission grants and web storage are keyed by.
    func serveDotNsContent(onPhase: @escaping ProductLoadPhaseHandler) async throws -> URL {
        let content = try await productOpening.loader.open(
            configuration.productDomain,
            strategy: productOpening.strategy,
            policy: .untilCancelled,
            onPhase: onPhase
        )

        logger.debug("SPA(rust): '\(configuration.productDomain)' serves \(content.archive.localURL.path)")

        let productURL = try await serve(content)
        return configuration.page.applied(to: productURL)
    }

    /// A direct URL is only named by its manifest, so a label without records still opens, unnamed.
    func productResolution() async -> RuntimeProductResolution {
        if let servedContent {
            return .dotNs(servedContent)
        }

        guard case .directURL = configuration.contentSource else { return .unserved(nil) }

        return await .unserved(try? productOpening.resolver.resolve(configuration.productDomain))
    }

    func serve(_ content: ProductContent) async throws -> URL {
        let productURL = try await serveFiles(at: content.archive.localURL)
        servedContent = content
        return productURL
    }

    /// Serves the App's files at `contentURL` under the page's base domain,
    /// which is what permission grants and web storage are keyed by.
    func serveFiles(at contentURL: URL) async throws -> URL {
        let domain = configuration.productDomain
        let schemeHandler = ProductScriptSchemeHandler(
            productId: domain,
            entryRelativePath: ProductBundle.indexHTML,
            productFileProvider: DotNsFileProvider(contentURL: contentURL)
        )

        guard let productURL = schemeHandler.getProductUrl() else {
            throw ScriptExecutorError.scriptNotFound(productId: domain)
        }

        // Don't publish a handler to the shared proxy after dispose ran.
        try checkNotDisposed()
        await schemeHandlerProxy.setHandler(schemeHandler)

        return productURL
    }

    func checkNotDisposed() throws {
        guard !disposed else { throw CancellationError() }
    }
}
