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
    case walletLocked
    case walletSelectionChanged
}

/// Vends the process-wide ``TrUAPIHostRuntime``. Product executions open off
/// the single shared runtime, so its authentication and core services are
/// shared across every SPA and chat product.
protocol TrUAPIHostRuntimeProviding: AnyObject, Sendable {
    var coreStorage: HostCoreStorageBackend { get }
    var secretStorage: HostSecretStorageBackend { get }

    /// Join the selected wallet activation without owning its cancellation.
    func sharedRuntime() async throws -> TrUAPIHostRuntime
    func lockWallet()

    /// Anchor the host's core confirmations (signing, permission prompts) to
    /// the given view. Until it is attached, host-level prompts deny.
    @MainActor func setPresentationView(_ view: ControllerBackedProtocol)
}

/// Shares one runtime and one readiness task across every product and SSO caller.
final class TrUAPIHostRuntimeProvider: TrUAPIHostRuntimeProviding, @unchecked Sendable {
    private let chainRegistry: ChainRegistryProtocol
    private let walletSecrets: NativeWalletSecretProvider
    private let installationKeyIdStore: InstallationKeyIdStoring
    private let settingsManager: SettingsManagerProtocol
    let coreStorage: HostCoreStorageBackend
    let secretStorage: HostSecretStorageBackend
    private let confirmationRouterFacade: ProductRoutersFacadeProtocol
    private let tldProvider: DotNsTldProviding
    private let logger: LoggerProtocol
    private let databaseDirectory: () throws -> String
    private let contactDataProviderFactory: ChatContactDataProviderMaking

    private let lock = NSLock()
    private var cachedRuntime: TrUAPIHostRuntime?
    private var readiness: (walletId: String, task: Task<TrUAPIHostRuntime, Error>)?
    private var isLocked = false
    private var selectionObservers: [NSObjectProtocol] = []
    private var contactsChangeNotifier: ContactsChangeNotifier?

    init(
        chainRegistry: ChainRegistryProtocol,
        entropyManager: RootEntropyManaging,
        settingsManager: SettingsManagerProtocol,
        coreStorage: HostCoreStorageBackend,
        secretStorage: HostSecretStorageBackend,
        installationKeyIdStore: InstallationKeyIdStoring = InstallationKeyIdStore(),
        confirmationRouterFacade: ProductRoutersFacadeProtocol,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared,
        databaseDirectory: @escaping () throws -> String = { try TrUAPIHostRuntimeProvider.coreDatabaseDirectory() },
        contactDataProviderFactory: ChatContactDataProviderMaking = ChatContactDataProviderFactory(),
        logger: LoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.installationKeyIdStore = installationKeyIdStore
        walletSecrets = TrUAPIWalletSecretProvider(
            entropyManager: entropyManager,
            installationKeyIdStore: installationKeyIdStore
        )
        self.settingsManager = settingsManager
        self.coreStorage = coreStorage
        self.secretStorage = secretStorage
        self.confirmationRouterFacade = confirmationRouterFacade
        self.tldProvider = tldProvider
        self.logger = logger
        self.databaseDirectory = databaseDirectory
        self.contactDataProviderFactory = contactDataProviderFactory
        selectionObservers = [
            NotificationCenter.default.addObserver(
                forName: InstallationKeyIdStore.willChangeNotification,
                object: nil,
                queue: nil
            ) { [weak self] _ in
                self?.lockWallet()
            },
            NotificationCenter.default.addObserver(
                forName: InstallationKeyIdStore.didChangeNotification,
                object: nil,
                queue: nil
            ) { [weak self] _ in
                self?.lock.withLock { self?.isLocked = false }
            }
        ]
    }

    deinit {
        selectionObservers.forEach(NotificationCenter.default.removeObserver)
        readiness?.task.cancel()
    }

    @MainActor
    func setPresentationView(_ view: ControllerBackedProtocol) {
        confirmationRouterFacade.setPresentationView(view)
    }

    func lockWallet() {
        lock.withLock {
            isLocked = true
            readiness?.task.cancel()
            readiness = nil
            cachedRuntime?.lockWallet()
        }
    }

    func sharedRuntime() async throws -> TrUAPIHostRuntime {
        let pending = try lock.withLock { () throws -> Task<TrUAPIHostRuntime, Error> in
            try Task.checkCancellation()
            guard !isLocked else { throw TrUAPIRuntimeConfigError.walletLocked }
            guard let walletId = installationKeyIdStore.getInstallationKeyId() else {
                throw RootEntropyManagerError.noEntropyFound
            }
            if let readiness, readiness.walletId == walletId {
                return readiness.task
            }
            readiness?.task.cancel()
            cachedRuntime?.lockWallet()
            let runtime = try cachedRuntime ?? makeRuntime()
            cachedRuntime = runtime
            let liteUsername = settingsManager.string(for: .username)
            let task = Task { [weak self] in
                do {
                    try Task.checkCancellation()
                    try await runtime.activateWallet(walletId: walletId, liteUsername: liteUsername)
                    try Task.checkCancellation()
                    return runtime
                } catch {
                    self?.lock.withLock {
                        if !Task.isCancelled { self?.readiness = nil }
                    }
                    throw error
                }
            }
            readiness = (walletId, task)
            return task
        }
        let runtime = try await pending.value
        try Task.checkCancellation()
        try lock.withLock {
            guard !isLocked, !pending.isCancelled,
                  readiness?.walletId == installationKeyIdStore.getInstallationKeyId() else {
                throw TrUAPIRuntimeConfigError.walletSelectionChanged
            }
        }
        return runtime
    }

    private func makeRuntime() throws -> TrUAPIHostRuntime {
        let runtimeConfig = try Self.makeRuntimeConfig(
            chainRegistry: chainRegistry,
            networkSuffix: tldProvider.currentTldOrError(),
            databaseDirectory: databaseDirectory()
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
            coreStorage: coreStorage,
            secretStorage: secretStorage,
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
            dataProviderFactory: contactDataProviderFactory,
            logger: logger,
            onSnapshot: { [weak contactsBridge] contacts in
                contactsBridge?.update(contacts: contacts)
            },
            onRemoval: { [weak runtime] in
                runtime?.notifyContactsChanged()
            }
        )
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
