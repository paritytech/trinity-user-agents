import Foundation
import IssueMonitoring
import Products
import TrUAPIHost
import UIKitExt

/// Rust SPA runtime: resolves the product content, publishes the scheme
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
    private let executionModel: RustRuntimeEnvironment.ExecutionModel
    private let configuration: SPAConfiguration
    private let contentLoader: ProductContentLoading
    private let schemeHandlerProxy: SchemeHandlerProxy
    private let logger: LoggerProtocol
    private let telemetry: HostLoadTelemetry
    private let hostSessionId: UUID
    private let openedAt: Int?

    private var engine: JSEngineProtocol?
    private var engineMonitor: JSEngineMonitor?
    private var started = false
    private var disposed = false
    private var servedContent: DotNsResolvedContent?
    private var pageLoadReported = false

    init(
        executionModel: RustRuntimeEnvironment.ExecutionModel,
        configuration: SPAConfiguration,
        contentLoader: ProductContentLoading,
        schemeHandlerProxy: SchemeHandlerProxy,
        logger: LoggerProtocol,
        telemetry: HostLoadTelemetry,
        hostSessionId: UUID = UUID()
    ) {
        self.executionModel = executionModel
        self.configuration = configuration
        self.contentLoader = contentLoader
        self.schemeHandlerProxy = schemeHandlerProxy
        self.logger = logger
        self.telemetry = telemetry
        self.hostSessionId = hostSessionId
        openedAt = telemetry.now()
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

        do {
            return try await serveFirstPage(with: engine)
        } catch {
            // An open that failed before its page could load is the page failing.
            if !(error is CancellationError) {
                reportPageLoad(.error, at: telemetry.now())
            }
            throw error
        }
    }

    private func serveFirstPage(with engine: JSEngineProtocol) async throws -> URL {
        let initialURL: URL =
            switch configuration.contentSource {
            case .dotNs:
                try await prepareDotNsContent()
            case let .directURL(url):
                url
            case let .bundled(directory):
                try await configuration.page.applied(to: serveFiles(at: directory))
            #if DEBUG
                case let .devServer(url, _):
                    url
            #endif
            }

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

        telemetry.record { origin, now in
            .webViewCreated(
                hostSessionID: hostSessionId,
                origin: origin,
                truapiHostVersion: truapiHostVersion(),
                monotonicMilliseconds: now
            )
        }

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

        if started, !pageLoadReported {
            reportPageLoad(.abandoned, at: telemetry.now())
        }

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

    func useLatestBuild() async throws {
        try checkNotDisposed()

        guard case .dotNs = configuration.contentSource, let servedContent else { return }

        let served = try await contentLoader.open(configuration.productDomain, kind: .app, policy: .untilCancelled)
            .archive
        try checkNotDisposed()

        guard served != servedContent else { return }

        _ = try await serve(served)
    }

    func servedBuild() -> DotNsResolvedContent? {
        servedContent
    }

    nonisolated func asPageOpen<R>(_ operation: () async throws -> R) async rethrows -> R {
        try await telemetry.withContext(session: hostSessionId, trigger: .open, operation: operation)
    }

    nonisolated func pageLoadTimestamp() -> Int? {
        telemetry.now()
    }

    /// Only the first page counts: a reload or a later navigation is not the product opening.
    func didFinishPageLoad(loaded: Bool, at finishedAt: Int) {
        guard engine != nil, !disposed else { return }

        reportPageLoad(loaded ? .ok : .error, at: finishedAt)
    }
}

private extension SPARustRuntime {
    func prepareDotNsContent() async throws -> URL {
        let domain = configuration.productDomain

        // Bytes come from the executable's own subname; the origin stays the base domain, which
        // is what permission grants and web storage are keyed by.
        let served = try await telemetry.withContext(session: hostSessionId, trigger: .open) {
            try await contentLoader.open(domain, kind: .app, policy: .untilCancelled)
        }.archive

        logger.debug("SPA(rust): '\(domain)' content resolved to \(served.localURL.path)")

        let productURL = try await serve(served)
        return configuration.page.applied(to: productURL)
    }

    func serve(_ content: DotNsResolvedContent) async throws -> URL {
        let productURL = try await serveFiles(at: content.localURL)
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

    func reportPageLoad(_ outcome: HostEventOutcome, at finishedAt: Int?) {
        guard let openedAt, let finishedAt, !pageLoadReported else { return }
        pageLoadReported = true

        telemetry.record { origin, now in
            .webViewPageLoaded(
                hostSessionID: hostSessionId,
                productID: configuration.productId.rawValue,
                outcome: outcome,
                durationMilliseconds: finishedAt - openedAt,
                origin: origin,
                monotonicMilliseconds: now
            )
        }
    }

    func checkNotDisposed() throws {
        guard !disposed else { throw CancellationError() }
    }
}
