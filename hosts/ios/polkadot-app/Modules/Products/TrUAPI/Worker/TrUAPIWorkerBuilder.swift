import Foundation
import BulletinChain
import ChainRegistry
import DesignSystem
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
    private let environment: @Sendable () throws -> RustRuntimeEnvironment
    private let contentLoader: any ProductContentLoading
    private let dotNsResolver: any DotNsResolverProtocol
    private let productFileProvider: any ChatProductFileProviding
    private let logger: LoggerProtocol

    init(
        environment: @escaping @Sendable () throws -> RustRuntimeEnvironment,
        contentLoader: any ProductContentLoading,
        dotNsResolver: any DotNsResolverProtocol,
        productFileProvider: any ChatProductFileProviding,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.environment = environment
        self.contentLoader = contentLoader
        self.dotNsResolver = dotNsResolver
        self.productFileProvider = productFileProvider
        self.logger = logger
    }

    func makeRuntime(
        productId: ProductId,
        context: ProductWorkerContext,
        attempt: NativeWorkerAttempt
    ) async throws -> TrUAPIWorkerRuntime {
        let (source, files) = try await workerSource(for: productId)

        let engine = try ChatProductEngineFactory.makeContext(
            source: source,
            productFileProvider: files,
            logger: logger
        )

        return try TrUAPIWorkerRuntime(
            productUrl: engine.productUrl,
            executionModel: environment().makeWorkerExecution(
                productId: productId,
                routers: context.routers,
                chatMessaging: context.chat,
                attempt: attempt
            ),
            engineFactory: engine.engineFactory,
            logger: logger
        )
    }

    /// The product's worker, resolved through dotNS, and the files it is
    /// served from.
    func workerSource(
        for productId: ProductId
    ) async throws -> (ProductWorkerSource, any ProductFileProviding) {
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

extension TrUAPIWorkerManager {
    static func make(
        runtimeProvider: any TrUAPIHostRuntimeProviding,
        flowState: SPAFlowState,
        productFileProvider: any ChatProductFileProviding,
        gameReminders: any ProductGameReminderScheduling,
        themeManager: any ThemeManagerProtocol,
        logger: CoreEventLoggerProtocol
    ) -> TrUAPIWorkerManager {
        let runtime: @Sendable () throws -> TrUAPIHostRuntime = { [weak runtimeProvider] in
            guard let runtimeProvider else { throw CancellationError() }
            return try runtimeProvider.sharedRuntime()
        }
        let builder = TrUAPIWorkerBuilder(
            environment: {
                try RustRuntimeEnvironment(
                    runtime: runtime(),
                    chainRegistry: ChainRegistryFacade.sharedRegistry,
                    notificationScheduler: ProductNotificationScheduler.shared,
                    gameReminders: gameReminders,
                    ipfsFetcher: IpfsFetcher(ipfsBaseURL: AppConfig.KnownIPFS.main),
                    hostProvider: flowState.hostProvider,
                    themeManager: themeManager,
                    logger: logger
                )
            },
            contentLoader: flowState.contentLoader,
            dotNsResolver: flowState.dotNsResolver,
            productFileProvider: productFileProvider,
            logger: logger
        )
        return TrUAPIWorkerManager(
            build: { try await builder.makeRuntime(productId: $0, context: $1, attempt: $2) },
            runtime: runtime,
            presentationFallback: { [weak runtimeProvider] in runtimeProvider?.presentationView }
        )
    }
}
