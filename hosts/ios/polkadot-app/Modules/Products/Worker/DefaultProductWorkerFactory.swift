import Foundation
import Keystore_iOS
import KeyDerivation
import Products
import ChainRegistry

enum DefaultProductWorkerFactoryError: Error {
    case noWorker(ProductId)
    case dependenciesUnavailable
    case invalidNativeApi
}

/// Boots one product's headless native worker for the ``ProductWorkerManager``.
///
/// This is the single place the native worker is assembled — the same wiring the
/// chat bot used inline before unification, now sourced by product id so SPA,
/// operations and chat all keep the one shared instance alive.
final class DefaultProductWorkerFactory: ProductWorkerFactory, @unchecked Sendable {
    private let productResolver: ProductResolving
    private let dotNsResolver: DotNsResolverProtocol
    private let productFileProvider: ChatProductFileProviding
    private let nativeScriptsFactory: ChatScriptsMaking
    private let chainRegistry: ChainRegistryProtocol
    private let usernameStorage: UsernameStoring
    private let hostProvider: ProductHostProviding
    private let notificationService: UserNotificationServicing
    private let entropyManager: RootEntropyManaging
    private let settingsManager: SettingsManagerProtocol
    private let substrateStorageFacade: StorageFacadeProtocol
    private let accountManager: ProductsAccountManaging
    private let vrfRepo: BandersnatchManagerRepositoryProtocol
    private let workerOperations: ProductWorkerOperating
    private let logger: LoggerProtocol

    init(
        productResolver: ProductResolving,
        dotNsResolver: DotNsResolverProtocol,
        productFileProvider: ChatProductFileProviding,
        chainRegistry: ChainRegistryProtocol,
        usernameStorage: UsernameStoring,
        hostProvider: ProductHostProviding,
        accountManager: ProductsAccountManaging,
        workerOperations: ProductWorkerOperating,
        nativeScriptsFactory: ChatScriptsMaking = ChatNativeRuntimeScriptsFactory(),
        notificationService: UserNotificationServicing = UserNotificationService.shared,
        entropyManager: RootEntropyManaging = RootEntropyManager.shared,
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        substrateStorageFacade: StorageFacadeProtocol = SubstrateDataStorageFacade.shared,
        vrfRepo: BandersnatchManagerRepositoryProtocol = .shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productResolver = productResolver
        self.dotNsResolver = dotNsResolver
        self.productFileProvider = productFileProvider
        self.chainRegistry = chainRegistry
        self.usernameStorage = usernameStorage
        self.hostProvider = hostProvider
        self.accountManager = accountManager
        self.workerOperations = workerOperations
        self.nativeScriptsFactory = nativeScriptsFactory
        self.notificationService = notificationService
        self.entropyManager = entropyManager
        self.settingsManager = settingsManager
        self.substrateStorageFacade = substrateStorageFacade
        self.vrfRepo = vrfRepo
        self.logger = logger
    }

    func startWorker(productId: ProductId) async throws -> ProductWorkerRunning {
        let resolved = await resolve(productId)

        guard let source = workerSource(for: resolved) else {
            throw DefaultProductWorkerFactoryError.noWorker(productId)
        }

        await warmWorkerArchive(of: resolved)

        let engineContext = try ChatProductEngineFactory.makeContext(
            source: source,
            productFileProvider: productFileProvider,
            logger: logger
        )

        let nativeApi = try makeNativeApi(productId: productId)

        let scriptExecutor = ProductsScriptExecutor(
            productUrl: engineContext.productUrl,
            scriptsFactory: nativeScriptsFactory,
            engineFactory: engineContext.engineFactory,
            logger: logger
        )

        try await scriptExecutor.initializeBot(nativeApi: nativeApi.api)

        return ProductScriptWorker(
            scriptExecutor: scriptExecutor,
            nativeApi: nativeApi.api,
            routers: nativeApi.routers
        )
    }
}

private extension DefaultProductWorkerFactory {
    func makeNativeApi(productId: ProductId) throws -> (api: ProductsNativeApi, routers: ProductRoutersFacadeProtocol) {
        guard let truApiDependencies: TruApiDependenciesLocator = RootDependencyLocator.getDependency() else {
            throw DefaultProductWorkerFactoryError.dependenciesUnavailable
        }

        let resourceKeyManager = ProductResourceKeyManager(
            keychain: Keychain(),
            userDefaults: SharedContainerGroup.userDefaults
        )

        let sponsorFactory = try HostTransactionSponsorFactory(
            accountManager: accountManager,
            resourceKeyManager: resourceKeyManager,
            chainRegistry: chainRegistry,
            keyResolver: vrfRepo.keyResolver(),
            logger: logger
        )

        let nativeApiFactory = ProductsNativeApiFactory(
            chainRegistry: chainRegistry,
            usernameStorage: usernameStorage,
            localStorage: ProductsLocalStorage(productId: productId, settingsManager: settingsManager),
            notificationService: notificationService,
            entropyManager: entropyManager,
            dependencyLocator: truApiDependencies,
            accountManager: accountManager,
            resourceKeyManager: resourceKeyManager,
            sponsorFactory: sponsorFactory,
            substrateStorageFacade: substrateStorageFacade,
            hostProvider: hostProvider,
            workerOperations: workerOperations
        )

        let routers = ProductRoutersFacade.worker()
        let api = nativeApiFactory.makeApi(productId: productId, routers: routers)

        guard let concrete = api as? ProductsNativeApi else {
            throw DefaultProductWorkerFactoryError.invalidNativeApi
        }

        return (concrete, routers)
    }

    /// Mirrors ``ProductBotProvider`` resolution: a malformed manifest yields no
    /// worker, a name that cannot be read degrades to the legacy shape.
    func resolve(_ productId: ProductId) async -> ResolvedProduct {
        do {
            return try await productResolver.resolve(productId)
        } catch {
            logger.error("Worker resolve fell back to legacy for \(productId): \(error)")
            return .legacy(id: productId)
        }
    }

    func workerSource(for resolved: ResolvedProduct) -> ProductWorkerSource? {
        ProductWorkerSource.published(for: resolved, serving: .chat)
            ?? ProductWorkerSource.installedByHand(for: resolved) {
                productFileProvider.manualScriptEntryPath(productId: $0)
            }
    }

    func warmWorkerArchive(of resolved: ResolvedProduct) async {
        guard let published = ProductWorkerSource.published(for: resolved, serving: .chat) else { return }

        do {
            _ = try await dotNsResolver.resolveToLocalURL(dotNsName: published.contentId)
        } catch {
            logger.error("Failed to warm the worker archive \(published.contentId): \(error)")
        }
    }
}
