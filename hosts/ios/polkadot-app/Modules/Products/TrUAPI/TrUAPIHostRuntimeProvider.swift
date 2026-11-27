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

/// Shares one ``TrUAPIHostRuntime`` across the dashboard session's products.
protocol TrUAPIHostRuntimeProviding: AnyObject, Sendable, ProductWorkerReloading {
    /// Return the shared runtime, building and activating its local session on
    /// first use. Subsequent calls return the cached instance.
    func sharedRuntime() throws -> TrUAPIHostRuntime

    /// Anchor the host's core confirmations (signing, permission prompts) to
    /// the given view. Until it is attached, host-level prompts deny.
    @MainActor func setPresentationView(_ view: ControllerBackedProtocol)

    /// Stop this session's workers even when a consumer outlives sign-out.
    func shutdownProductWorkers()
}

/// Builds one ``TrUAPIHostRuntime`` from host identity + the stack the build
/// bundles + the local session secret, activates the local session once, and
/// caches it. Built off the main thread by ``setup()``, or on first use
/// before that, so startup is not blocked.
final class TrUAPIHostRuntimeProvider: TrUAPIHostRuntimeProviding, @unchecked Sendable {
    private let chainRegistry: ChainRegistryProtocol
    private let entropyManager: RootEntropyManaging
    private let settingsManager: SettingsManagerProtocol
    private let coreStorage: TrUAPILocalStoring
    private let confirmationRouterFacade: ProductRoutersFacadeProtocol
    private let workerEngine: any NativeWorkerEngine
    private let backedUpInstallation = TrUAPIBackedUpInstallation()
    private let tldProvider: DotNsTldProviding
    private let fundingHandler: FundingCoreEventHandling
    private let remoteConfig: RemoteConfigObserving?
    private let appliedConfig: () -> RemoteAppConfig?
    private let deviceEncryptionKeys: DeviceEncryptionKeyManager
    private let logger: CoreEventLoggerProtocol

    private let lock = NSLock()
    private var cachedRuntime: TrUAPIHostRuntime?
    private var sessionEnded = false
    private var contactsChangeNotifier: ContactsChangeNotifier?
    private var fundingProvidersObservation: Task<Void, Never>?

    /// Set once at startup, after the payment engines are assembled. Installed on the runtime when
    /// it is built, or right away if it already is.
    private var paymentsBridge: RustPaymentsBridge?

    init(
        chainRegistry: ChainRegistryProtocol,
        entropyManager: RootEntropyManaging,
        settingsManager: SettingsManagerProtocol,
        coreStorage: TrUAPILocalStoring,
        confirmationRouterFacade: ProductRoutersFacadeProtocol,
        workerEngine: any NativeWorkerEngine,
        fundingHandler: FundingCoreEventHandling,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared,
        remoteConfig: RemoteConfigObserving? = nil,
        appliedConfig: @escaping () -> RemoteAppConfig? = { AppConfigProvider.shared.getRemoteConfig() },
        deviceEncryptionKeys: DeviceEncryptionKeyManager = .shared,
        logger: CoreEventLoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.entropyManager = entropyManager
        self.settingsManager = settingsManager
        self.coreStorage = coreStorage
        self.confirmationRouterFacade = confirmationRouterFacade
        self.workerEngine = workerEngine
        self.fundingHandler = fundingHandler
        self.tldProvider = tldProvider
        self.remoteConfig = remoteConfig
        self.appliedConfig = appliedConfig
        self.deviceEncryptionKeys = deviceEncryptionKeys
        self.logger = logger
    }

    deinit {
        fundingProvidersObservation?.cancel()
    }

    @MainActor
    func setPresentationView(_ view: ControllerBackedProtocol) {
        confirmationRouterFacade.setPresentationView(view)
    }

    func reloadWorker(of productId: ProductId) {
        let runtime = lock.withLock { sessionEnded ? nil : cachedRuntime }
        runtime?.reloadWorker(productId: productId)
    }

    func shutdownProductWorkers() {
        let runtime = lock.withLock {
            sessionEnded = true
            return cachedRuntime
        }
        runtime?.shutdownProductWorkers()
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

    /// Builds the runtime now, off the caller's thread, rather than on first
    /// use: the core resumes the funding sessions a provider was still working
    /// on when it starts. Called once, after payments are attached.
    func setup() async throws -> TrUAPIHostRuntime {
        try await Task.detached { [self] in try sharedRuntime() }.value
    }

    func sharedRuntime() throws -> TrUAPIHostRuntime {
        lock.lock()
        defer { lock.unlock() }

        guard !sessionEnded else { throw CancellationError() }
        if let cachedRuntime {
            return cachedRuntime
        }

        let secret = try entropyManager.fetchRootEntropy()
        let networkSuffix = try tldProvider.currentTldOrError()
        let handedOverKey = try deviceEncryptionKeys.keyToHandOver()
        let runtimeConfig = try Self.makeRuntimeConfig(
            secret: secret,
            liteUsername: settingsManager.string(for: .username),
            networkSuffix: networkSuffix,
            databaseDirectory: Self.coreDatabaseDirectory(),
            deviceEncryptionKey: handedOverKey
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
            backedUpInstallation: backedUpInstallation,
            encryptedStorage: TrUAPIEncryptedStorage(keychain: Keychain()),
            logger: logger
        )

        let runtime = try TrUAPIHostRuntime(bridge: bridge, runtimeConfig: runtimeConfig, workerEngine: workerEngine)
        if handedOverKey != nil {
            deviceEncryptionKeys.forgetHandedOverKey()
        }
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
        // host lists: the ones `funding_config` names.
        runtime.setFunding(AppFundingHostBridge(handler: fundingHandler))
        bridge.onWorkerCrashing { [weak fundingHandler] productId in
            fundingHandler?.workerCrashing(productId: productId)
        }
        listFundingProviders(on: runtime)
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
        return runtime
    }
}

extension TrUAPIHostRuntimeProvider {
    /// Assemble the immutable host-wide config. Every chain fact the core uses
    /// comes from the stack this build bundles, ``TrUAPIBundledStack/current``.
    /// `networkSuffix` is the dotNS TLD the app's own built-in accounts derive
    /// from, which names the products backing the pinned Pocket cards.
    static func makeRuntimeConfig(
        secret: Data,
        liteUsername: String?,
        networkSuffix: String,
        databaseDirectory: String,
        deviceEncryptionKey: Data?
    ) throws -> HostRuntimeConfig {
        let bundle = Bundle.main
        let version = bundle.appVersion.map { version in
            bundle.appBuild.map { "\(version) (\($0))" } ?? version
        }

        return try HostRuntimeConfig(
            hostName: "Polkadot App",
            hostVersion: version,
            platformType: "ios",
            platformVersion: UIDevice.current.systemVersion,
            stack: TrUAPIBundledStack.current.load(),
            databaseDirectory: databaseDirectory,
            pinnedPocketCards: AssetPinnedPocketCards(tld: networkSuffix).cards(),
            localSessionSecret: secret,
            localSessionLiteUsername: liteUsername,
            deviceEncryptionKey: deviceEncryptionKey
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
    /// List the providers `funding_config` names once per launch: the config
    /// already applied, or else the first one remote config delivers, as it
    /// usually lands after the runtime is built. Later refreshes are not
    /// followed until known products carry the funding providers.
    func listFundingProviders(on runtime: TrUAPIHostRuntime) {
        if let config = appliedConfig() {
            apply(publishedNames: config.fundingProviders, on: runtime)
            return
        }
        apply(publishedNames: [], on: runtime)

        guard let stream = remoteConfig?.remoteConfigStream() else { return }

        fundingProvidersObservation?.cancel()
        fundingProvidersObservation = Task { [weak self, weak runtime] in
            do {
                for try await config in stream {
                    guard let self, let runtime, !Task.isCancelled else { return }

                    apply(publishedNames: config.fundingProviders, on: runtime)
                    return
                }
            } catch {
                self?.logger.error("[truapi] funding providers stopped following remote config: \(error)")
            }
        }
    }

    func apply(publishedNames: [String], on runtime: TrUAPIHostRuntime) {
        runtime.setFundingProviderConfig(publishedNames: publishedNames)
    }
}
