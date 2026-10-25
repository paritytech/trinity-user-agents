import Foundation
import FoundationExt
import UIKit
import Keystore_iOS
import PolkadotUI
import Products
import ChainRegistry
import BulletinChain
import DesignSystem

/// Creates ``ProductBot`` instances for a given product.
///
/// The native path is a thin adapter over the shared ``ProductWorkerManager`` —
/// the worker itself is assembled by ``DefaultProductWorkerFactory``. The rust
/// path is a chat seam onto the product's one worker, which
/// ``TrUAPIWorkerManager`` runs.
final class ProductBotFactory {
    private let productFileProvider: ChatProductFileProviding
    private let settingsManager: SettingsManagerProtocol
    private let runtimeProvider: TrUAPIHostRuntimeProviding
    private let workers: @Sendable () -> (any TrUAPIWorkerManaging)?
    private let workerManager: ProductWorkerManaging
    private let productImages: ProductImageSources
    private let logger: LoggerProtocol

    init(
        productFileProvider: ChatProductFileProviding,
        runtimeProvider: TrUAPIHostRuntimeProviding,
        workers: @Sendable @escaping () -> (any TrUAPIWorkerManaging)?,
        workerManager: ProductWorkerManaging,
        productImages: ProductImageSources,
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productFileProvider = productFileProvider
        self.settingsManager = settingsManager
        self.runtimeProvider = runtimeProvider
        self.workers = workers
        self.workerManager = workerManager
        self.productImages = productImages
        self.logger = logger
    }

    func create(resolved: ResolvedProduct) -> ProductBot? {
        guard servesChat(resolved) else { return nil }

        let product = resolved.product
        let description = resolved.description?.nilIfEmpty
        let images = productImages.resolver(contentId: { resolved.contentId(for: .worker) })
        let resolveImage = WidgetImageResolver.cached { await images.resolve($0) }

        if settingsManager.isTrUAPIRuntimeEnabled, let workers = workers() {
            let runtime = TrUAPIChatHandler(
                productId: product.identifier,
                workers: workers,
                logger: logger
            )
            return ProductBot(
                product: product,
                description: description,
                runtime: runtime,
                resolveImage: resolveImage,
                logger: logger
            )
        }

        let runtime = ManagedChatRuntime(productId: product.identifier, manager: workerManager)
        return ProductBot(
            product: product,
            description: description,
            runtime: runtime,
            resolveImage: resolveImage,
            logger: logger
        )
    }
}

private extension ProductBotFactory {
    /// A bot is a chat surface, so a worker that declares `includes.chat: false`
    /// gets none. That is a valid background-only worker, and a bot for it
    /// would take a reference on a modality the product never declared.
    ///
    /// A worker declaring `includes.chat: false` is driven by ProductWorkerManager
    /// as a plain Rust worker, never adapted to the Chat protocol.
    func workerSource(for resolved: ResolvedProduct) -> ProductWorkerSource? {
        if let worker = resolved.executables.worker {
            guard worker.includesChat else { return nil }

            return ProductWorkerSource(contentId: worker.identifier, entryRelativePath: worker.entrypoint)
        }

        return productFileProvider.manualScriptEntryPath(productId: resolved.id).map {
            ProductWorkerSource(contentId: resolved.id, entryRelativePath: $0)
        }
    }

    /// Builds one rust chat runtime (product execution + localhost ws-bridge)
    /// off the shared runtime. The local session lives on the shared runtime;
    /// a transient provider failure only fails this create.
    func createRustRuntime(product: Product, source: ProductWorkerSource) throws -> ChatRuntimeProtocol {
        let runtime = try runtimeProvider.sharedRuntime()

        let engineContext = try ChatProductEngineFactory.makeContext(
            source: source,
            productFileProvider: productFileProvider,
            logger: logger
        )

        let routers = ProductRoutersFacade.worker()
        let productId = product.identifier

        return ChatRustRuntime(
            productUrl: engineContext.productUrl,
            makeExecutionModel: { [hostProvider, chainRegistry, logger] chatMessaging in
                let rustEnvironment = RustRuntimeEnvironment(
                    runtime: runtime,
                    chainRegistry: chainRegistry,
                    notificationScheduler: ProductNotificationScheduler.shared,
                    ipfsFetcher: IpfsFetcher(ipfsBaseURL: AppConfig.KnownIPFS.main),
                    hostProvider: hostProvider,
                    themeManager: ThemeManager.shared,
                    logger: logger
                )
                return try rustEnvironment.makeChatExecution(
                    productId: productId,
                    routers: routers,
                    chatMessaging: chatMessaging
                )
            },
            routers: routers,
            engineFactory: engineContext.engineFactory,
            logger: logger
        )
    }
}
