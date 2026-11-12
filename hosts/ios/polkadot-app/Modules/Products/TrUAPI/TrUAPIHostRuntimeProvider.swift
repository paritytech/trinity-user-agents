import AsyncExtensions
import Foundation
import UIKit
import UIKitExt
import TrUAPIHost
import ChainRegistry
import Foundation_iOS
import FoundationExt
import Products
import SubstrateSdk
import KeyDerivation
import Keystore_iOS

/// Runtime configuration error raised while assembling the shared host config.
enum TrUAPIRuntimeConfigError: Error {
    case missingGenesisHash(chain: String)
    /// The runtime was built without its funding service.
    case fundingServiceMissing
}

/// Vends the process-wide ``TrUAPIHostRuntime``. Product executions open off
/// the single shared runtime, so its authentication and core services are
/// shared across every SPA and chat product.
protocol TrUAPIHostRuntimeProviding: AnyObject, Sendable {
    /// Return the shared runtime, building and activating its local session on
    /// first use. Subsequent calls return the cached instance.
    func sharedRuntime() throws -> TrUAPIHostRuntime

    /// Anchor the host's core confirmations (signing, permission prompts) to
    /// the given view. Until it is attached, host-level prompts deny.
    @MainActor func setPresentationView(_ view: ControllerBackedProtocol)

    /// The view the host's own prompts present from, once attached.
    @MainActor var presentationView: ControllerBackedProtocol? { get }

    /// Attach what runs product workers when the core's reference ledger asks
    /// for them. Called once at startup, before the runtime is first built.
    func attach(workerManager: any TrUAPIWorkerManaging)
}

/// Lazily builds one ``TrUAPIHostRuntime`` from host identity + people/bulletin
/// genesis hashes + the local session secret, activates the local session
/// once, and caches it. Lazy so startup is not blocked and the runtime is only
/// assembled once chains are synced and a session secret exists.
final class TrUAPIHostRuntimeProvider: TrUAPIHostRuntimeProviding, @unchecked Sendable {
    private let chainRegistry: ChainRegistryProtocol
    private let entropyManager: RootEntropyManaging
    private let settingsManager: SettingsManagerProtocol
    private let coreStorage: TrUAPILocalStoring
    private let confirmationRouterFacade: ProductRoutersFacadeProtocol
    private let backedUpInstallation = TrUAPIBackedUpInstallation()
    private let tldProvider: DotNsTldProviding
    private let fundingServiceSlot = FundingServiceSlot()
    private let bundledProducts: BundledProducts
    private let remoteConfig: RemoteConfigObserving?
    private let appliedConfig: () -> RemoteAppConfig?
    private let logger: LoggerProtocol

    private let lock = NSLock()
    private var cachedRuntime: TrUAPIHostRuntime?
    private var contactsChangeNotifier: ContactsChangeNotifier?
    private var fundingProvidersObservation: Task<Void, Never>?

    /// Set once at startup, before any product opens. The runtime is built on
    /// first use, which is long after, so the manager is in place by then.
    private var workerManager: (any TrUAPIWorkerManaging)?

    /// Set once at startup, after the payment engines are assembled. Installed on the runtime when
    /// it is built, or right away if it already is.
    private var paymentsBridge: RustPaymentsBridge?

    init(
        chainRegistry: ChainRegistryProtocol,
        entropyManager: RootEntropyManaging,
        settingsManager: SettingsManagerProtocol,
        coreStorage: TrUAPILocalStoring,
        confirmationRouterFacade: ProductRoutersFacadeProtocol,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared,
        bundledProducts: BundledProducts = .app,
        remoteConfig: RemoteConfigObserving? = nil,
        appliedConfig: @escaping () -> RemoteAppConfig? = { AppConfigProvider.shared.getRemoteConfig() },
        logger: LoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.entropyManager = entropyManager
        self.settingsManager = settingsManager
        self.coreStorage = coreStorage
        self.confirmationRouterFacade = confirmationRouterFacade
        self.tldProvider = tldProvider
        self.bundledProducts = bundledProducts
        self.remoteConfig = remoteConfig
        self.appliedConfig = appliedConfig
        self.logger = logger
    }

    deinit {
        fundingProvidersObservation?.cancel()
    }

    @MainActor
    func setPresentationView(_ view: ControllerBackedProtocol) {
        confirmationRouterFacade.setPresentationView(view)
    }

    @MainActor
    var presentationView: ControllerBackedProtocol? {
        confirmationRouterFacade.productsRouter.presentationView
    }

    func attach(workerManager: any TrUAPIWorkerManaging) {
        lock.lock()
        defer { lock.unlock() }

        self.workerManager = workerManager
    }

    /// Serve top-ups, payments and balance to the core from `paymentsSupport`. Approvals and
    /// privacy warnings are presented like the core's own host-level prompts.
    func attach(paymentsSupport: PaymentsSupport) {
        backedUpInstallation.attach(paymentsSupport.coinageService.backedUpInstallationProvider)
        let productsRouter = confirmationRouterFacade.productsRouter
        let bridge = RustPaymentsBridge(
            payments: ProductPayments(
                support: paymentsSupport,
                approvalRequester: PaymentApprovalRequesterFactory.create(router: productsRouter),
                privacyConfirmer: PaymentPrivacyConfirmer(router: productsRouter),
                recyclingStrategy: CoinageRecyclingStrategyStore.shared
            ),
            logger: logger
        )

        lock.lock()
        defer { lock.unlock() }

        guard paymentsBridge == nil else { return }
        paymentsBridge = bridge
        if let cachedRuntime {
            bridge.install(on: cachedRuntime)
        }
    }

    func sharedRuntime() throws -> TrUAPIHostRuntime {
        lock.lock()
        defer { lock.unlock() }

        if let cachedRuntime {
            return cachedRuntime
        }

        let secret = try entropyManager.fetchRootEntropy()
        let networkSuffix = try tldProvider.currentTldOrError()
        let runtimeConfig = try Self.makeRuntimeConfig(
            chainRegistry: chainRegistry,
            secret: secret,
            liteUsername: settingsManager.string(for: .username),
            networkSuffix: networkSuffix,
            databaseDirectory: Self.coreDatabaseDirectory()
        )

        let chainConnections = TrUAPIV2SpecConnectionPool(
            chainRegistry: chainRegistry,
            applicationHandler: ApplicationHandler(),
            dateProvider: NowDateProvider(),
            logger: logger
        )

        let bridge = RustHostRuntimeBridge(
            chainRegistry: chainRegistry,
            coreStorage: coreStorage,
            chainConnections: chainConnections,
            confirmationPresenter: TrUAPIConfirmationPresenter(routerFacade: confirmationRouterFacade),
            workerManager: workerManager,
            backedUpInstallation: backedUpInstallation,
            encryptedStorage: TrUAPIEncryptedStorage(keychain: Keychain()),
            logger: logger
        )

        let runtime = try TrUAPIHostRuntime(bridge: bridge, runtimeConfig: runtimeConfig)
        bridge.attach(runtime)
        // Before any product execution opens, so a product never sees the
        // window where the host lists no contacts.
        let contactsBridge = AppContactsHostBridge(
            repositoryFactory: ChatContactRepositoryFactory(),
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            routerFacade: confirmationRouterFacade
        )
        runtime.setContacts(contactsBridge)
        runtime.setScanner(AppScannerHostBridge(
            routerFacade: confirmationRouterFacade,
            visibility: ProductVisibility(application: UIApplication.shared)
        ))
        paymentsBridge?.install(on: runtime)
        // Also before any execution opens, so a product's Funding request
        // never answers unsupported. The core offers only the providers the
        // host lists: the ones `funding_config` names, then the ones it ships.
        let fundingService = FundingService(runtime: runtime, logger: logger)
        runtime.setFunding(AppFundingHostBridge(handler: fundingService))
        bridge.onWorkerCrashing { [weak fundingService] productId in
            fundingService?.workerCrashing(productId: productId)
        }
        followFundingProviders(on: runtime)
        contactsChangeNotifier = ContactsChangeNotifier(
            dataProviderFactory: ChatContactDataProviderFactory(),
            logger: logger,
            onSnapshot: { [weak contactsBridge] contacts in
                contactsBridge?.update(contacts: contacts)
            },
            onRemoval: { [weak runtime] in
                runtime?.notifyContactsChanged()
            }
        )
        try runtime.activateLocalSession(secret: secret, liteUsername: settingsManager.string(for: .username))

        cachedRuntime = runtime
        fundingServiceSlot.fill(fundingService)
        return runtime
    }
}

extension TrUAPIHostRuntimeProvider: FundingServiceProviding {
    func fundingService() throws -> FundingServiceProtocol {
        _ = try sharedRuntime()
        guard let service = fundingServiceSlot.service else {
            throw TrUAPIRuntimeConfigError.fundingServiceMissing
        }
        return service
    }

    func builtFundingService() -> AsyncStream<FundingServiceProtocol> {
        fundingServiceSlot.built()
    }
}

extension TrUAPIHostRuntimeProvider {
    /// Assemble the immutable host-wide config. Genesis hashes are fetched from
    /// the registry and must resolve; a missing hash fails explicitly rather
    /// than degrading. `networkSuffix` is the dotNS TLD the core derives the
    /// wallet's reserved identities under, so it has to be the one the app's own
    /// built-in accounts derive from. Exposed for testing the genesis-validation
    /// seam.
    static func makeRuntimeConfig(
        chainRegistry: ChainRegistryProtocol,
        secret: Data,
        liteUsername: String?,
        networkSuffix: String,
        databaseDirectory: String
    ) throws -> HostRuntimeConfig {
        let peopleChain = try chainRegistry.getChainOrError(for: AppConfig.Chains.peopleChain)
        let bulletinChain = try chainRegistry.getChainOrError(for: AppConfig.Chains.bulletInChain)
        let assetHubChain = try chainRegistry.getChainOrError(for: AppConfig.Chains.assethubChain)

        guard let peopleGenesisHex = peopleChain.explicitGenesisHash else {
            throw TrUAPIRuntimeConfigError.missingGenesisHash(chain: "people")
        }
        guard let bulletinGenesisHex = bulletinChain.explicitGenesisHash else {
            throw TrUAPIRuntimeConfigError.missingGenesisHash(chain: "bulletin")
        }
        // Product manifests are read from the dotNS contracts on Asset Hub, so
        // a missing hash here refuses every cross-product `trustedProducts`
        // grant indistinguishably from the other product granting nothing.
        // Fail explicitly, like its siblings, rather than passing all-zero.
        guard let assetHubGenesisHex = assetHubChain.explicitGenesisHash else {
            throw TrUAPIRuntimeConfigError.missingGenesisHash(chain: "assetHub")
        }

        let bundle = Bundle.main
        let version = bundle.appVersion.map { version in
            bundle.appBuild.map { "\(version) (\($0))" } ?? version
        }

        return try HostRuntimeConfig(
            hostName: "Polkadot App",
            hostVersion: version,
            platformType: "ios",
            platformVersion: UIDevice.current.systemVersion,
            peopleChainGenesisHash: Data(hexString: peopleGenesisHex),
            bulletinChainGenesisHash: Data(hexString: bulletinGenesisHex),
            assetHubChainGenesisHash: Data(hexString: assetHubGenesisHex),
            networkSuffix: networkSuffix,
            databaseDirectory: databaseDirectory,
            pinnedPocketCards: AssetPinnedPocketCards(tld: networkSuffix).cards(),
            localSessionSecret: secret,
            localSessionLiteUsername: liteUsername
        )
    }

    /// The core database directory under Application Support, created if
    /// needed and excluded from backup: a durable-transaction ledger restored
    /// onto another device would act on transactions that already settled.
    static func coreDatabaseDirectory(fileManager: FileManager = .default) throws -> String {
        var directory = try fileManager
            .url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            .appendingPathComponent("truapi", isDirectory: true)
        try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try directory.setResourceValues(values)
        return directory.path
    }
}

private extension TrUAPIHostRuntimeProvider {
    /// List the providers known now, then list them again on every remote
    /// config refresh, which the core ignores when it names the same ones.
    /// Remote config usually lands after the runtime is built, so startup
    /// never waits for it.
    func followFundingProviders(on runtime: TrUAPIHostRuntime) {
        let bundled = bundledProducts.fundingProviders()
        apply(publishedNames: appliedConfig()?.fundingProviders ?? [], bundled: bundled, on: runtime)

        guard let stream = remoteConfig?.remoteConfigStream() else { return }

        fundingProvidersObservation?.cancel()
        fundingProvidersObservation = Task { [weak self, weak runtime] in
            do {
                for try await config in stream {
                    guard let self, let runtime, !Task.isCancelled else { return }

                    apply(publishedNames: config.fundingProviders, bundled: bundled, on: runtime)
                }
            } catch {
                self?.logger.error("[truapi] funding providers stopped following remote config: \(error)")
            }
        }
    }

    func apply(publishedNames: [String], bundled: [FundingProviderEntry], on runtime: TrUAPIHostRuntime) {
        do {
            try runtime.setFundingProviderConfig(publishedNames: publishedNames, bundled: bundled)
        } catch {
            logger.error("[truapi] the funding providers were refused: \(error)")
        }
    }
}
