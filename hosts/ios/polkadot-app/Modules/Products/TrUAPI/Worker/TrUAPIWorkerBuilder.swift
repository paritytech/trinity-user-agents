import Foundation
@preconcurrency import Products
import TrUAPIHost

enum TrUAPIWorkerError: Error, CustomStringConvertible {
    case noWorker(ProductId)

    var description: String {
        switch self {
        case let .noWorker(productId): "\(productId) publishes no worker"
        }
    }
}

/// Assembles one product's worker: its archive, its Worker execution, and the
/// headless engine its entry module runs in.
struct TrUAPIWorkerBuilder: TrUAPIWorkerBuilding {
    private let environment: @Sendable () throws -> RustRuntimeEnvironment
    private let contentLoader: any ProductContentLoading
    private let dotNsResolver: any DotNsResolverProtocol
    private let productFileProvider: any ChatProductFileProviding
    private let bundledProducts: BundledProducts
    private let logger: LoggerProtocol

    init(
        environment: @escaping @Sendable () throws -> RustRuntimeEnvironment,
        contentLoader: any ProductContentLoading,
        dotNsResolver: any DotNsResolverProtocol,
        productFileProvider: any ChatProductFileProviding,
        bundledProducts: BundledProducts = .app,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.environment = environment
        self.contentLoader = contentLoader
        self.dotNsResolver = dotNsResolver
        self.productFileProvider = productFileProvider
        self.bundledProducts = bundledProducts
        self.logger = logger
    }

    func makeRuntime(
        productId: ProductId,
        context: ProductWorkerContext,
        boot: WorkerBootTrace?
    ) async throws -> TrUAPIWorkerRuntime {
        let resolveStartedAt = boot?.now()
        let (source, files) = try await resolveWorkerSource(for: productId, boot: boot)
        boot?.add(.resolve, since: resolveStartedAt)

        let engine = try ChatProductEngineFactory.makeContext(
            source: source,
            productFileProvider: files,
            logger: logger
        )

        let executionStartedAt = boot?.now()
        let executionModel = try environment().makeWorkerExecution(
            productId: productId,
            routers: context.routers,
            chatMessaging: context.chat,
            hostSessionId: boot?.hostSessionId
        )
        boot?.add(.executionOpen, since: executionStartedAt)

        return TrUAPIWorkerRuntime(
            productUrl: engine.productUrl,
            executionModel: executionModel,
            engineFactory: engine.engineFactory,
            boot: boot,
            logger: logger
        )
    }

    private func resolveWorkerSource(
        for productId: ProductId,
        boot: WorkerBootTrace?
    ) async throws -> (ProductWorkerSource, any ProductFileProviding) {
        guard let boot else { return try await workerSource(for: productId) }

        return try await boot.withSession { try await workerSource(for: productId) }
    }

    /// The worker the app ships for the product, read from its own files with
    /// no dotNS lookup, and the files it is served from.
    func workerSource(
        for productId: ProductId
    ) async throws -> (ProductWorkerSource, any ProductFileProviding) {
        if let bundled = bundledProducts.product(productId), bundled.hasWorker() {
            return (.bundled(bundled), DotNsFileProvider(contentURL: bundled.directory))
        }

        // Its 1 s and 2 s backoffs fit inside the manager's startup window.
        let resolved = try await contentLoader.openWorker(productId, policy: .attempts(3))
        return try await (workerSource(for: resolved, productId: productId), productFileProvider)
    }

    /// The product's published worker, or the script installed by hand through
    /// debug settings, which is what makes a product drivable before it
    /// publishes anything. Which modalities that worker serves is the caller's
    /// to check: a holder must not take a reference on a modality the worker
    /// never declared.
    private func workerSource(
        for resolved: ResolvedProduct,
        productId: ProductId
    ) async throws -> ProductWorkerSource {
        if let published = ProductWorkerSource.published(for: resolved) {
            // Fetched before the engine boots: the scheme handler reads the
            // archive off disk, and a page loaded before it is there fails as a
            // missing module rather than waiting.
            _ = try await dotNsResolver.resolveToLocalURL(dotNsName: published.contentId.rawValue)

            return published
        }

        guard let installed = ProductWorkerSource.installedByHand(for: resolved, entryPath: {
            productFileProvider.manualScriptEntryPath(productId: $0)
        }) else {
            throw TrUAPIWorkerError.noWorker(productId)
        }

        return installed
    }
}
