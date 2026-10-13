import Foundation
import Products
import Individuality
import KeyDerivation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery
import SubstrateOperation
import ChainRegistry
import StructuredConcurrency
import BackgroundExecution

extension SSStoreAllowanceManager {
    static func create(
        chainRegistry: ChainRegistryProtocol,
        userStorageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        substrateStorageFacade: StorageFacadeProtocol = SubstrateDataStorageFacade.shared,
        entropyManager: RootEntropyManaging = RootEntropyManager.shared,
        walletRepo: WalletManagerRepositoryProtocol = .shared,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared
    ) -> SSStoreAllowanceManager? {
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

        let allowanceRepository = AllowanceRepositoryFactory(storageFacade: userStorageFacade)
            .createStatementStoreRepository()
        let accounting = StatementStoreSlotAccountant(repository: allowanceRepository)

        let serialQueue = SerialOperationQueue()

        let timeProvider = ChainTimeProvider(
            chainId: chatChain.chainId,
            chainRegistry: chainRegistry,
            storageRequestFactory: storageRequestFactory
        )
        let logger = Logger.shared

        let originPersonProvider = ChainOriginPersonProvider(
            chainId: chatChain.chainId,
            chainRegistry: chainRegistry,
            keyResolver: keyResolver
        )

        let slotInfoProvider = StatementStoreSlotInfoProvider(
            chainId: chatChain.chainId,
            chainRegistry: chainRegistry,
            storageRequestFactory: storageRequestFactory,
            networkSuffixProvider: tldProvider,
            resourcesParameters: ResourcesParametersFacade.shared,
            chainTimeProvider: timeProvider,
            originPersonProvider: originPersonProvider,
            accounting: accounting,
            logger: logger
        )

        let submitter = SlotAssignmentSubmitter(monitorFactory: monitorFactory)

        let allocator = StatementStoreSlotAllocator(
            chainId: chatChain.chainId,
            originFactory: originFactory,
            submitter: submitter,
            slotInfoProvider: slotInfoProvider,
            serialQueue: serialQueue
        )

        let renewer = StatementStoreSlotRenewer(
            chainId: chatChain.chainId,
            slotInfoProvider: slotInfoProvider,
            accounting: accounting,
            submitter: submitter,
            originFactory: originFactory,
            chainTimeProvider: timeProvider,
            serialQueue: serialQueue,
            issueDiagnostics: .make(reporter: IssueReportingFacade.shared),
            logger: logger
        )

        let manager = SSStoreAllowanceManager(
            repository: allowanceRepository,
            allocator: allocator,
            slotInfoProvider: slotInfoProvider,
            renewer: renewer,
            backgroundExecutor: ConnectionRetainingExecutor(provider: chainRegistry),
            issueDiagnostics: .make(flow: "statement-store-allowance", reporter: IssueReportingFacade.shared)
        )

        return manager
    }
}
