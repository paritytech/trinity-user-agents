import Foundation
import UIKit
import UIKitExt
import TrUAPIHost
import ChainRegistry
import Products
import SubstrateSdk
import KeyDerivation
import Keystore_iOS
import Coinage

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
    func sharedRuntime() throws -> TrUAPIHostRuntime

    /// Settings must bind reads and writes to the account that rendered the row.
    func automaticUploadScope() throws -> TrUAPIAutomaticUploadScope?
    func observeAuthorizationScope() -> AsyncStream<UUID>
    func setAuthorizationAvailable(_ available: Bool)

    func setCoinageAvailable(_ available: Bool)

    /// Anchor the host's core confirmations (signing, permission prompts) to
    /// the given view. Until it is attached, host-level prompts deny.
    @MainActor func setPresentationView(_ view: ControllerBackedProtocol)
}

struct TrUAPIAutomaticUploadScope: Equatable, Sendable {
    let generation: UUID
    let rootPublicKey: Data
    let bulletinGenesis: Data
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
    private let tldProvider: DotNsTldProviding
    private let logger: LoggerProtocol
    private let coinageAdapter: TrUAPINativeCoinage

    private let lock = NSLock()
    private var cachedRuntime: TrUAPIHostRuntime?
    private var cachedBulletinGenesis: Data?
    private var contactsChangeNotifier: ContactsChangeNotifier?
    private var authorizationAvailable = false
    private var authorizationGeneration = UUID()
    private var authorizationObservers: [UUID: AsyncStream<UUID>.Continuation] = [:]

    init(
        chainRegistry: ChainRegistryProtocol,
        entropyManager: RootEntropyManaging,
        settingsManager: SettingsManagerProtocol,
        coreStorage: TrUAPILocalStoring,
        confirmationRouterFacade: ProductRoutersFacadeProtocol,
        coinageService: any CoinageServicing,
        storageFacade: StorageFacadeProtocol,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared,
        logger: LoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.entropyManager = entropyManager
        self.settingsManager = settingsManager
        self.coreStorage = coreStorage
        self.confirmationRouterFacade = confirmationRouterFacade
        self.tldProvider = tldProvider
        self.logger = logger
        // CoinageService is assembled for this wallet/chain. Live configuration may not relabel that instance.
        let boundScope = try? Self.makeNativeCoinageScope(chainRegistry: chainRegistry, entropyManager: entropyManager)
        coinageAdapter = TrUAPINativeCoinage(
            service: coinageService,
            storageFacade: storageFacade,
            scope: {
                guard let boundScope else {
                    throw HostRejection.Rejected(reason: "Native Coinage wallet session unavailable")
                }
                let currentScope = try Self.makeNativeCoinageScope(
                    chainRegistry: chainRegistry,
                    entropyManager: entropyManager
                )
                guard currentScope == boundScope else {
                    throw HostRejection.Rejected(reason: "Native Coinage wallet session unavailable")
                }
                return boundScope
            },
            confirmationPresenter: TrUAPIConfirmationPresenter(routerFacade: confirmationRouterFacade)
        )
    }

    deinit {
        // The runtime can outlive its provider. It retains the adapter, not an authorization lease.
        coinageAdapter.setAvailable(false)
        for observer in authorizationObservers.values {
            observer.finish()
        }
    }

    @MainActor
    func setPresentationView(_ view: ControllerBackedProtocol) {
        confirmationRouterFacade.setPresentationView(view)
    }

    func setCoinageAvailable(_ available: Bool) {
        coinageAdapter.setAvailable(available)
    }

    func setAuthorizationAvailable(_ available: Bool) {
        lock.withLock {
            guard authorizationAvailable != available else { return }
            authorizationAvailable = available
            authorizationGeneration = UUID()
            for observer in authorizationObservers.values {
                observer.yield(authorizationGeneration)
            }
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
            coinageInstanceId: AppConfig.Coinage.instanceId,
            databaseDirectory: Self.coreDatabaseDirectory()
        )

        let chainConnections = TrUAPIChainConnectionPool(
            chainRegistry: chainRegistry,
            logger: logger
        )

        let bridge = RustHostRuntimeBridge(
            chainRegistry: chainRegistry,
            coreStorage: coreStorage,
            chainConnections: chainConnections,
            confirmationPresenter: TrUAPIConfirmationPresenter(routerFacade: confirmationRouterFacade),
            chatFiles: TrUAPINativeChatFiles.shared,
            logger: logger
        )

        // Always register native custody, including while the service is unavailable.
        // Temporary unavailability must never opt this host into Rust purse storage.
        let runtime = try TrUAPIHostRuntime(bridge: bridge, runtimeConfig: runtimeConfig, nativeWallet: coinageAdapter)
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
        try runtime.activateLocalSession(secret: secret, liteUsername: settingsManager.string(for: .username))

        cachedRuntime = runtime
        cachedBulletinGenesis = runtimeConfig.bulletinChainGenesisHash
        return runtime
    }

    func observeAuthorizationScope() -> AsyncStream<UUID> {
        let id = UUID()
        let (stream, continuation) = AsyncStream<UUID>.makeStream(bufferingPolicy: .bufferingNewest(1))
        continuation.onTermination = { [weak self] _ in
            _ = self?.lock.withLock { self?.authorizationObservers.removeValue(forKey: id) }
        }
        lock.withLock {
            authorizationObservers[id] = continuation
            continuation.yield(authorizationGeneration)
        }
        return stream
    }

    func automaticUploadScope() throws -> TrUAPIAutomaticUploadScope? {
        guard lock.withLock({ authorizationAvailable }) else { return nil }
        let runtime = try sharedRuntime()
        return try lock.withLock {
            guard authorizationAvailable,
                  let bulletinGenesis = cachedBulletinGenesis,
                  let rootPublicKey = runtime.currentSessionPublicKey() else { return nil }
            // A retained provider must never label its old runtime as a newly selected wallet.
            let wallet = DynamicDerivedWallet(derivationPath: nil, entropyManager: entropyManager)
            guard try wallet.getRawPublicKey() == rootPublicKey else { return nil }
            return TrUAPIAutomaticUploadScope(
                generation: authorizationGeneration,
                rootPublicKey: rootPublicKey,
                bulletinGenesis: bulletinGenesis
            )
        }
    }
}

extension TrUAPIHostRuntimeProvider {
    private static func makeNativeCoinageScope(
        chainRegistry: ChainRegistryProtocol,
        entropyManager: RootEntropyManaging
    ) throws -> NativeCoinageScope {
        let chain = try chainRegistry.getChainOrError(for: AppConfig.Assets.mainAsset.chainId)
        guard let genesis = chain.explicitGenesisHash else {
            throw TrUAPIRuntimeConfigError.missingGenesisHash(chain: "coinage")
        }
        let wallet = DynamicDerivedWallet(derivationPath: nil, entropyManager: entropyManager)
        return try NativeCoinageScope(
            rootPublicKey: wallet.getRawPublicKey(),
            genesisHash: Data(hexString: genesis),
            coinageInstanceId: AppConfig.Coinage.instanceId
        )
    }

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
        coinageInstanceId: UInt32,
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
            databaseDirectory: databaseDirectory,
            localSessionSecret: secret,
            localSessionLiteUsername: liteUsername,
            coinageInstanceId: coinageInstanceId
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
