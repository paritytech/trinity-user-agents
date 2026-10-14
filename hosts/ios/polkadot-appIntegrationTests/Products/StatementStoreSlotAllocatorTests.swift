@testable import polkadot_app
import XCTest
import Keystore_iOS
import NovaCrypto
import SubstrateSdk
import Operation_iOS
import ExtrinsicService
import KeyDerivation
import Individuality
import SDKLogger
import SubstrateStorageQuery
import SubstrateOperation
import Products
import ChainRegistry
import StructuredConcurrency

final class StatementStoreSlotAllocatorTests: XCTestCase {
    private let mnemonic = "city digital broken voice chef envelope swarm disagree claw fox friend casual"

    func testAssignSlotOnChain() async throws {
        let setupResult = try setupWallet()
        let storageFacade = SubstrateStorageTestFacade()
        let operationQueue = OperationQueue()

        let chainRegistry = ChainRegistryFacade.setupForIntegrationTest(
            with: storageFacade,
            logger: Logger.shared
        )

        let storageRequestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )

        let liteVrfManager = BandersnatchKeyManager.litePerson(for: "dot", entropyManager: setupResult.entropyManager)
        let fullVrfManager = BandersnatchKeyManager.fullPerson(for: "dot", entropyManager: setupResult.entropyManager)

        let keyResolver = BandersnatchKeyResolver(
            liteKeyManager: liteVrfManager,
            fullKeyManager: fullVrfManager
        )

        let originFactory = AsResourcesOriginFactory(
            wallet: setupResult.wallet,
            keyResolver: keyResolver,
            chainRegistry: chainRegistry,
            networkSuffixProvider: ChainNetworkSuffixProvider(
                chainId: KnownChainId.previewNetPeople,
                chainRegistry: chainRegistry,
                storageRequestFactory: storageRequestFactory
            )
        )

        let facade = ExtrinsicSubmissionMonitorFacade(
            chainRegistry: chainRegistry,
            substrateStorageFacade: storageFacade,
            operationQueue: operationQueue,
            logger: Logger.shared
        )

        let chain = try chainRegistry.getChainOrError(for: KnownChainId.previewNetPeople)
        let monitorFactory = try facade.createMonitorFactory(chain: chain)

        let timeProvider = ChainTimeProvider(
            chainId: KnownChainId.previewNetPeople,
            chainRegistry: chainRegistry,
            storageRequestFactory: storageRequestFactory
        )

        let allowanceRepository = AllowanceRepositoryFactory(storageFacade: UserDataStorageFacade.shared)
            .createStatementStoreRepository()
        let accounting = StatementStoreSlotAccountant(repository: allowanceRepository)
        let originPersonProvider = ChainOriginPersonProvider(
            chainId: KnownChainId.previewNetPeople,
            chainRegistry: chainRegistry,
            keyResolver: keyResolver
        )

        let slotInfoProvider = StatementStoreSlotInfoProvider(
            chainId: KnownChainId.previewNetPeople,
            chainRegistry: chainRegistry,
            storageRequestFactory: storageRequestFactory,
            networkSuffixProvider: ChainNetworkSuffixProvider(
                chainId: KnownChainId.previewNetPeople,
                chainRegistry: chainRegistry,
                storageRequestFactory: storageRequestFactory
            ),
            resourcesParameters: CachedResourcesParametersProvider(
                viewFunctionExecutor: ViewFunctionExecutor(
                    chainRegistry: chainRegistry,
                    operationQueue: operationQueue
                ),
                ttl: 0
            ),
            chainTimeProvider: timeProvider,
            originPersonProvider: originPersonProvider,
            accounting: accounting,
            logger: MockLogger()
        )

        let serialQueue = SerialOperationQueue()

        let allocator = StatementStoreSlotAllocator(
            chainId: KnownChainId.previewNetPeople,
            originFactory: originFactory,
            submitter: SlotAssignmentSubmitter(monitorFactory: monitorFactory),
            slotInfoProvider: slotInfoProvider,
            serialQueue: serialQueue
        )

        let holder = ProductAccountHolder(entropyManager: setupResult.entropyManager)
        let accountId = try holder.deriveAccount(ProductAccountId(productId: "browse.dot", derivationIndex: .index(0)))

        try await allocator.assignSlot(accountId: accountId, priority: .normal)
    }
}

// MARK: - Wallet Setup

private extension StatementStoreSlotAllocatorTests {
    struct WalletSetup {
        let wallet: WalletManaging
        let entropyManager: RootEntropyManaging
    }

    func setupWallet() throws -> WalletSetup {
        let keychain = InMemoryKeychain()
        let entropyManager = RootEntropyManager(
            keychain: keychain,
            installationKeyIdStore: InstallationKeyIdStore(userDefaults: .standard)
        )
        let manager = WalletSetupManager(
            mnemonicGenerator: IRMnemonicCreator(),
            mnemonicBackupHelper: MockMnemonicBackupHelper(),
            entropyManager: entropyManager,
            logger: Logger.shared
        )
        let mnemonicObj = try IRMnemonicCreator().mnemonic(fromList: mnemonic)
        try manager.createWallets(with: .init(mnemonic: mnemonicObj))
        return WalletSetup(
            wallet: DynamicDerivedWallet(derivationPath: "//wallet", entropyManager: entropyManager),
            entropyManager: entropyManager
        )
    }
}
