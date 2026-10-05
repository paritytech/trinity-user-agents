import Foundation
import UIKit
import UIKitExt
import TrUAPIHost
import ChainRegistry
import Products
import SubstrateSdk
import KeyDerivation
import Keystore_iOS

/// Runtime configuration error raised while assembling the shared host config.
enum TrUAPIRuntimeConfigError: Error {
    case missingGenesisHash(chain: String)
}

/// Vends the process-wide ``TrUAPIHostRuntime``. Product executions open off
/// the single shared runtime, so its authentication and core services are
/// shared across every SPA and chat product.
protocol TrUAPIHostRuntimeProviding: AnyObject, Sendable {
    /// Return the shared runtime, building and activating its local session on
    /// first use. Subsequent calls return the cached instance.
    func sharedRuntime() async throws -> TrUAPIHostRuntime
    func activeRuntimeForRecords() async throws -> TrUAPIHostRuntime?

    /// Anchor the host's core confirmations (signing, permission prompts) to
    /// the given view. Until it is attached, host-level prompts deny.
    @MainActor func setPresentationView(_ view: ControllerBackedProtocol)
    func workerChatRuntime(productId: String) async throws -> ChatRuntimeProtocol
}

/// Serializes wallet activation and keeps one runtime for the selected owner.
actor TrUAPIHostRuntimeProvider: TrUAPIHostRuntimeProviding {
    private let workerResolver: DotNsResolverProtocol
    private var workerEngines: TrUAPIWorkerEngines?
    private let chainRegistry: ChainRegistryProtocol
    private let entropyManager: RootEntropyManaging
    private let settingsManager: SettingsManagerProtocol
    private let walletSecrets: TrUAPIWalletSecrets
    private var walletId: String?
    private nonisolated(unsafe) let confirmationRouterFacade: ProductRoutersFacadeProtocol
    private let tldProvider: DotNsTldProviding
    private let logger: LoggerProtocol

    private var protectedDataLocked = false
    private var protectionObserver: TrUAPIWalletProtectionObserver?
    private var pendingRuntime: Task<TrUAPIHostRuntime, Error>?
    private var cachedRuntime: TrUAPIHostRuntime?
    private var contactsChangeNotifier: ContactsChangeNotifier?

    init(
        chainRegistry: ChainRegistryProtocol,
        workerResolver: DotNsResolverProtocol,
        entropyManager: RootEntropyManaging,
        settingsManager: SettingsManagerProtocol,
        confirmationRouterFacade: ProductRoutersFacadeProtocol,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared,
        logger: LoggerProtocol
    ) {
        self.workerResolver = workerResolver
        self.chainRegistry = chainRegistry
        self.entropyManager = entropyManager
        self.settingsManager = settingsManager
        walletSecrets = TrUAPIWalletSecrets(entropyManager: entropyManager)
        self.confirmationRouterFacade = confirmationRouterFacade
        self.tldProvider = tldProvider
        self.logger = logger
    }

    @MainActor
    func setPresentationView(_ view: ControllerBackedProtocol) {
        confirmationRouterFacade.setPresentationView(view)
    }

    func workerChatRuntime(productId: String) async throws -> ChatRuntimeProtocol {
        _ = try await sharedRuntime()
        guard let workerEngines else { throw ProductBotFactoryError.dependenciesUnavailable }
        return TrUAPIWorkerChatRuntime(productId: productId, engines: workerEngines)
    }

    func activeRuntimeForRecords() async throws -> TrUAPIHostRuntime? {
        guard !protectedDataLocked else { return nil }
        do {
            let runtime = try await sharedRuntime()
            return protectedDataLocked ? nil : runtime
        } catch {
            guard !protectedDataLocked else { return nil }
            throw error
        }
    }

    func sharedRuntime() async throws -> TrUAPIHostRuntime {
        if protectionObserver == nil {
            protectionObserver = TrUAPIWalletProtectionObserver { [weak self] available in
                Task { await self?.protectedDataChanged(available: available) }
            }
        }
        guard !protectedDataLocked else { throw HostRejection.Rejected(reason: "Wallet is locked") }
        guard let selectedWallet = InstallationKeyIdStore().getInstallationKeyId() else {
            throw RootEntropyManagerError.noEntropyFound
        }
        if let cachedRuntime, walletId == selectedWallet {
            return cachedRuntime
        }
        if let pendingRuntime {
            _ = try await pendingRuntime.value
            return try await sharedRuntime()
        }
        let task = Task { try await buildRuntime(selectedWallet: selectedWallet) }
        pendingRuntime = task
        defer { pendingRuntime = nil }
        return try await task.value
    }

    private func protectedDataChanged(available: Bool) async {
        protectedDataLocked = !available
        notifyRuntimeRecordsChanged()
        if !available {
            if let pendingRuntime { _ = try? await pendingRuntime.value }
            walletId = nil
            do { try await cachedRuntime?.lockWallet() }
            catch { logger.error("Rust wallet lock failed: \(error)") }
        } else if InstallationKeyIdStore().getInstallationKeyId() != nil {
            do { _ = try await sharedRuntime() }
            catch { logger.error("Rust wallet activation failed: \(error)") }
        }
    }

    private func buildRuntime(selectedWallet: String) async throws -> TrUAPIHostRuntime {
        try await cachedRuntime?.shutdown()
        cachedRuntime = nil
        workerEngines = nil
        contactsChangeNotifier = nil
        walletId = nil
        let networkSuffix = try tldProvider.currentTldOrError()
        let runtimeConfig = try Self.makeRuntimeConfig(
            chainRegistry: chainRegistry,
            networkSuffix: networkSuffix,
            databaseDirectory: Self.coreDatabaseDirectory()
        )

        let chainConnections = TrUAPIChainConnectionPool(
            engineResolver: { [chainRegistry] genesisHash in
                chainRegistry.getChainByGenesis(for: genesisHash.toHex()).flatMap { chain in
                    chainRegistry.getConnection(for: chain.chainId)
                }
            },
            logger: logger
        )

        let bridge = RustHostRuntimeBridge(
            chainRegistry: chainRegistry,
            secretStorage: TrUAPISecretStorage(),
            walletId: selectedWallet,
            permissionRequester: ProductPermissionRequester(router: confirmationRouterFacade.productsRouter),
            osPermissionAsker: OSPermissionAsker(),
            chainConnections: chainConnections,
            confirmationPresenter: TrUAPIConfirmationPresenter(routerFacade: confirmationRouterFacade),
            logger: logger
        )

        let runtime = try TrUAPIHostRuntime(bridge: bridge, walletSecrets: walletSecrets, runtimeConfig: runtimeConfig)
        bridge.attach(runtime)
        // Before any product execution opens, so a product never sees the
        // window where the host lists no contacts.
        let contactsBridge = AppContactsHostBridge(
            repositoryFactory: ChatContactRepositoryFactory(),
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            routerFacade: confirmationRouterFacade
        )
        runtime.setContacts(contactsBridge)
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
        let engines = TrUAPIWorkerEngines(host: runtime, resolver: workerResolver, logger: logger)
        guard runtime.setWorkerEngineHost(host: engines) else {
            throw HostRejection.Rejected(reason: "Worker engine already installed")
        }
        workerEngines = engines
        try await runtime.activateWallet(walletId: selectedWallet, liteUsername: settingsManager.string(for: .username))
        guard !protectedDataLocked, InstallationKeyIdStore().getInstallationKeyId() == selectedWallet else {
            try await runtime.shutdown()
            throw CancellationError()
        }
        walletId = selectedWallet

        cachedRuntime = runtime
        notifyRuntimeRecordsChanged()
        runtime.startStatementAllowanceRenewal()
        Task { [logger] in
            do { try await runtime.reconcileNotifications() }
            catch { logger.error("Rust notification reconciliation failed: \(error)") }
        }
        return runtime
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
        networkSuffix: String,
        databaseDirectory: String
    ) throws -> HostRuntimeConfig {
        let peopleChain = try chainRegistry.getChainOrError(for: AppConfig.Chains.usernameChain)
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

        let version = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String

        return try HostRuntimeConfig(
            hostName: "Polkadot App",
            hostVersion: version,
            platformType: "ios",
            platformVersion: UIDevice.current.systemVersion,
            peopleChainGenesisHash: Data(hexString: peopleGenesisHex),
            bulletinChainGenesisHash: Data(hexString: bulletinGenesisHex),
            assetHubChainGenesisHash: Data(hexString: assetHubGenesisHex),
            networkSuffix: networkSuffix,
            databaseDirectory: databaseDirectory
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

private final class TrUAPIWalletProtectionObserver {
    private let observers: [NSObjectProtocol]

    init(changed: @escaping @Sendable (Bool) -> Void) {
        observers = [
            NotificationCenter.default.addObserver(forName: UIApplication.protectedDataWillBecomeUnavailableNotification, object: nil, queue: nil) { _ in changed(false) },
            NotificationCenter.default.addObserver(forName: UIApplication.protectedDataDidBecomeAvailableNotification, object: nil, queue: nil) { _ in changed(true) }
        ]
    }

    deinit { observers.forEach(NotificationCenter.default.removeObserver) }
}
