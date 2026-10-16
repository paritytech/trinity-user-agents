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
import ChainStore
import ChainRegistry

final class BulletinSlotAllocatorTests: XCTestCase {
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
                chainId: AppConfig.Chains.usernameChain,
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

        let chainTimeProvider = ChainTimeProvider(
            chainId: AppConfig.Chains.bulletInChain,
            chainRegistry: chainRegistry,
            storageRequestFactory: storageRequestFactory
        )

        let allocator: AllowanceSlotAllocating = BulletinSlotAllocator(
            submissionChainId: AppConfig.Chains.usernameChain,
            slotInfoProvider: BulletInSlotInfoProvider(
                bulletInChainId: AppConfig.Chains.bulletInChain,
                peopleChainId: AppConfig.Chains.usernameChain,
                chainRegistry: chainRegistry,
                keyResolver: keyResolver,
                operationQueue: operationQueue,
                chainTimeProvider: chainTimeProvider,
                resourcesParameters: CachedResourcesParametersProvider(
                    viewFunctionExecutor: ViewFunctionExecutor(
                        chainRegistry: chainRegistry,
                        operationQueue: operationQueue
                    ),
                    ttl: 0
                ),
                networkSuffixProvider: ChainNetworkSuffixProvider(
                    chainId: AppConfig.Chains.usernameChain,
                    chainRegistry: chainRegistry,
                    storageRequestFactory: storageRequestFactory
                )
            ),
            originFactory: originFactory,
            submitter: SlotAssignmentSubmitter(monitorFactory: monitorFactory)
        )

        let holder = ProductAccountHolder(entropyManager: setupResult.entropyManager)
        let accountId = try holder.deriveAccount(ProductAccountId(productId: "browse.dot", derivationIndex: .index(0)))

        try await allocator.assignSlot(accountId: accountId, priority: .normal)
    }
}

// MARK: - Wallet Setup

private extension BulletinSlotAllocatorTests {
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
