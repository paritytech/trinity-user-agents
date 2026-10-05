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
    private let products: any ProductResolving
    private let dotNsResolver: any DotNsResolverProtocol
    private let productFileProvider: any ChatProductFileProviding
    private let logger: LoggerProtocol

    init(
        environment: @escaping @Sendable () throws -> RustRuntimeEnvironment,
        products: any ProductResolving,
        dotNsResolver: any DotNsResolverProtocol,
        productFileProvider: any ChatProductFileProviding,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.environment = environment
        self.products = products
        self.dotNsResolver = dotNsResolver
        self.productFileProvider = productFileProvider
        self.logger = logger
    }

    func makeRuntime(
        productId: ProductId,
        context: ProductWorkerContext,
        pocket: ProductPocketHostBridge
    ) async throws -> TrUAPIWorkerRuntime {
        let resolved = try? await products.resolve(productId)
        let source = try await workerSource(for: resolved, productId: productId)

        let engine = try ChatProductEngineFactory.makeContext(
            source: source,
            productFileProvider: productFileProvider,
            logger: logger
        )

        return try TrUAPIWorkerRuntime(
            productUrl: engine.productUrl,
            executionModel: environment().makeWorkerExecution(
                productId: productId,
                routers: context.routers,
                chatMessaging: context.chat,
                pocket: pocket
            ),
            engineFactory: engine.engineFactory,
            logger: logger
        )
    }

    /// The product's published worker, or the script installed by hand through
    /// debug settings, which is what makes a product drivable before it
    /// publishes anything. Which modalities that worker serves is the caller's
    /// to check: a holder must not take a reference on a modality the worker
    /// never declared.
    private func workerSource(
        for resolved: ResolvedProduct?,
        productId: ProductId
    ) async throws -> ProductWorkerSource {
        let resolved = resolved ?? .legacy(id: productId)

        if let published = ProductWorkerSource.published(for: resolved) {
            // Fetched before the engine boots: the scheme handler reads the
            // archive off disk, and a page loaded before it is there fails as a
            // missing module rather than waiting.
            _ = try await dotNsResolver.resolveToLocalURL(dotNsName: published.contentId)

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
