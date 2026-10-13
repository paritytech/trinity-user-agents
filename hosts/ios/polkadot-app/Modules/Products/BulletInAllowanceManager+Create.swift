import Foundation
import Products
import Individuality
import KeyDerivation
import Operation_iOS
import ChainRegistry
import SubstrateStorageQuery
import SubstrateSdk
import BackgroundExecution

extension BulletInAllowanceManager {
    static func create(
        chainRegistry: ChainRegistryProtocol,
        substrateStorageFacade: StorageFacadeProtocol = SubstrateDataStorageFacade.shared,
        entropyManager: RootEntropyManaging = RootEntropyManager.shared,
        walletRepo: WalletManagerRepositoryProtocol = .shared,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared
    ) -> BulletInAllowanceManager? {
        let operationQueue = OperationManagerFacade.sharedDefaultQueue

        guard
            let tld = try? tldProvider.currentTldOrError(),
            let wallet = try? walletRepo.main()
        else {
            return nil
        }

        let keyResolver = BandersnatchKeyResolver(
            liteKeyManager: BandersnatchKeyManager.litePerson(for: tld, entropyManager: entropyManager),
            fullKeyManager: BandersnatchKeyManager.fullPerson(for: tld, entropyManager: entropyManager)
        )

        let storageRequestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )

        let originFactory = AsResourcesOriginFactory(
            wallet: wallet,
            keyResolver: keyResolver,
            chainRegistry: chainRegistry,
            networkSuffixProvider: tldProvider
        )

        let extrinsicFacade = ExtrinsicSubmissionMonitorFacade(
            chainRegistry: chainRegistry,
            substrateStorageFacade: substrateStorageFacade,
            operationQueue: operationQueue
        )

        guard
            let chatChain = chainRegistry.getChain(for: AppConfig.Chains.chatChain),
            let monitorFactory = try? extrinsicFacade.createMonitorFactory(chain: chatChain)
        else {
            return nil
        }

        let chainTimeProvider = ChainTimeProvider(
            chainId: AppConfig.Chains.usernameChain,
            chainRegistry: chainRegistry,
            storageRequestFactory: storageRequestFactory
        )

        let infoProvider = BulletInSlotInfoProvider(
            bulletInChainId: AppConfig.Chains.bulletInChain,
            peopleChainId: AppConfig.Chains.usernameChain,
            chainRegistry: chainRegistry,
            keyResolver: keyResolver,
            operationQueue: operationQueue,
            chainTimeProvider: chainTimeProvider,
            resourcesParameters: ResourcesParametersFacade.shared,
            networkSuffixProvider: tldProvider
        )

        let allocator = BulletinSlotAllocator(
            submissionChainId: AppConfig.Chains.usernameChain,
            slotInfoProvider: infoProvider,
            originFactory: originFactory,
            submitter: SlotAssignmentSubmitter(monitorFactory: monitorFactory)
        )

        return BulletInAllowanceManager(
            infoProvider: infoProvider,
            allocator: allocator,
            backgroundExecutor: ConnectionRetainingExecutor(provider: chainRegistry),
            issueDiagnostics: .make(flow: "bulletin-allowance", reporter: IssueReportingFacade.shared),
            logger: Logger.shared
        )
    }
}
