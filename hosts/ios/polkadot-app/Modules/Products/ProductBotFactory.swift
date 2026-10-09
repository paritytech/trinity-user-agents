import Foundation
import UIKit
import Keystore_iOS
import Products
import ChainRegistry
import BulletinChain

/// Creates ``ProductBot`` instances for a given product.
///
/// The native path is a thin adapter over the shared ``ProductWorkerManager`` —
/// the worker itself is assembled by ``DefaultProductWorkerFactory``. The rust
/// path still builds its own runtime off the shared TrUAPI runtime.
final class ProductBotFactory {
    private let productFileProvider: ChatProductFileProviding
    private let chainRegistry: ChainRegistryProtocol
    private let hostProvider: ProductHostProviding
    private let settingsManager: SettingsManagerProtocol
    private let runtimeProvider: TrUAPIHostRuntimeProviding
    private let workerManager: ProductWorkerManaging
    private let gameReminders: ProductGameReminderScheduling
    private let logger: LoggerProtocol

    init(
        productFileProvider: ChatProductFileProviding,
        chainRegistry: ChainRegistryProtocol,
        hostProvider: ProductHostProviding,
        runtimeProvider: TrUAPIHostRuntimeProviding,
        workerManager: ProductWorkerManaging,
        gameReminders: ProductGameReminderScheduling,
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productFileProvider = productFileProvider
        self.chainRegistry = chainRegistry
        self.hostProvider = hostProvider
        self.settingsManager = settingsManager
        self.runtimeProvider = runtimeProvider
        self.workerManager = workerManager
        self.gameReminders = gameReminders
        self.logger = logger
    }

    func create(resolved: ResolvedProduct) -> ProductBot? {
        guard let source = workerSource(for: resolved) else { return nil }

        let product = resolved.product

        if settingsManager.isTrUAPIRuntimeEnabled {
            do {
                let runtime = try createRustRuntime(product: product, source: source)
                return ProductBot(product: product, runtime: runtime, logger: logger)
            } catch {
                logger.error(
                    "Rust runtime creation failed for \(product.identifier): \(error); falling back to native"
                )
            }
        }

        let runtime = ManagedChatRuntime(productId: product.identifier, manager: workerManager)
        return ProductBot(product: product, runtime: runtime, logger: logger)
    }
}

enum ProductBotFactoryError: Error {
    case dependenciesUnavailable
}

private extension ProductBotFactory {
    /// A worker comes from a published manifest, or from a script installed by hand through debug
    /// settings. Nil means the product ships no chat surface and gets no bot.
    ///
    /// A bot is a chat surface, so a worker that declares `includes.chat: false` gets none — that
    /// is a valid background-only worker, and iOS has nothing else to run it on.
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

        let rustEnvironment = RustRuntimeEnvironment(
            runtime: runtime,
            chainRegistry: chainRegistry,
            notificationScheduler: ProductNotificationScheduler.shared,
            gameReminders: gameReminders,
            ipfsFetcher: IpfsFetcher(ipfsBaseURL: AppConfig.KnownIPFS.main),
            hostProvider: hostProvider,
            logger: logger
        )

        let engineContext = try ChatProductEngineFactory.makeContext(
            source: source,
            productFileProvider: productFileProvider,
            logger: logger
        )

        let routers = ProductRoutersFacade.worker()
        let productId = product.identifier

        return ChatRustRuntime(
            productUrl: engineContext.productUrl,
            makeExecutionModel: { [rustEnvironment] chatMessaging in
                try rustEnvironment.makeChatExecution(
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
