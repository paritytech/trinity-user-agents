import Foundation
import BulletinChain
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
struct TrUAPIWorkerBuilder {
    let contentLoader: any ProductContentLoading
    let dotNsResolver: any DotNsResolverProtocol
    let productFileProvider: any ChatProductFileProviding

    func makeRuntime(
        productId: ProductId,
        context: ProductWorkerContext,
        attempt: NativeWorkerAttempt,
        services: ProductExecutionServices
    ) async throws -> TrUAPIWorkerRuntime {
        let (source, files) = try await workerSource(for: productId)

        let engine = try ChatProductEngineFactory.makeContext(
            source: source,
            productFileProvider: files,
            logger: services.logger
        )

        return try TrUAPIWorkerRuntime(
            productUrl: engine.productUrl,
            executionModel: services.makeExecution(
                productId: productId,
                routers: context.routers,
                purpose: .worker(chatMessaging: context.chat, attempt: attempt)
            ),
            engineFactory: engine.engineFactory,
            logger: services.logger
        )
    }

    /// The product's worker, resolved through dotNS, and the files it is
    /// served from.
    func workerSource(
        for productId: ProductId
    ) async throws -> (ProductWorkerSource, any ProductFileProviding) {
        // Its 1 s and 2 s backoffs fit inside the core's startup deadline.
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
