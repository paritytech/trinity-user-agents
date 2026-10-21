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
import Products
import ChainRegistry

final class PGASSlotAllocatorTests: XCTestCase {
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

        let originFactory = PGasOriginFactory(
            keyResolver: keyResolver,
            chainRegistry: chainRegistry,
            storageRequestFactory: storageRequestFactory,
            networkSuffixProvider: ChainNetworkSuffixProvider(
                chainId: AppConfig.Chains.assethubChain,
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

        let chain = try chainRegistry.getChainOrError(for: AppConfig.Chains.assethubChain)
        let monitorFactory = try facade.createMonitorFactory(chain: chain)

        let chainTimeProvider = ChainTimeProvider(
            chainId: chain.chainId,
            chainRegistry: chainRegistry,
            storageRequestFactory: storageRequestFactory
        )

        let slotInfoProvider = PGASSlotInfoProvider(
            chainId: chain.chainId,
            peopleChainId: KnownChainId.previewNetPeople,
            chainRegistry: chainRegistry,
            storageRequestFactory: storageRequestFactory,
            networkSuffixProvider: ChainNetworkSuffixProvider(
                chainId: chain.chainId,
                chainRegistry: chainRegistry,
                storageRequestFactory: storageRequestFactory
            ),
            keyResolver: keyResolver,
            chainTimeProvider: chainTimeProvider
        )

        let allocator: AllowanceSlotAllocating = PGASSlotAllocator(
            submissionChainId: chain.chainId,
            originChainId: KnownChainId.previewNetPeople,
            originFactory: originFactory,
            submitter: SlotAssignmentSubmitter(monitorFactory: monitorFactory),
            slotInfoProvider: slotInfoProvider
        )

        let accountHolder = ProductAccountHolder(entropyManager: setupResult.entropyManager)
        let accountId = try accountHolder.deriveStatementStoreAccount(for: "test-product1").getRawPublicKey()

        try await allocator.assignSlot(accountId: accountId, priority: .normal)
    }
}

// MARK: - Wallet Setup

private extension PGASSlotAllocatorTests {
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
