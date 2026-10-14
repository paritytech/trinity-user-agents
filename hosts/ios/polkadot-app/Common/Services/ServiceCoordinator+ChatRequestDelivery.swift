import DurableTransactions
import Foundation
import Individuality
import KeyDerivation
import Operation_iOS
import Products
import SubstrateSdk
import SubstrateStorageQuery

extension ServiceCoordinator {
    /// Registers the notification slot claim domain with the shared durable engine — before the engine starts —
    /// and returns the allocator chat requests are funded through.
    static func createNotificationAllocator(
        durable: DurableTxServices,
        keyResolver: BandersnatchKeyResolving,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared,
        walletRepo: WalletManagerRepositoryProtocol = .shared
    ) throws -> NotificationStatementAccountAllocating {
        let dependencies = makeNotificationSlotDependencies(keyResolver: keyResolver, tldProvider: tldProvider)

        let originFactory = try AsResourcesOriginFactory(
            wallet: walletRepo.main(),
            keyResolver: keyResolver,
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            networkSuffixProvider: tldProvider
        )

        registerNotificationSlotDomain(in: durable, dependencies: dependencies, originFactory: originFactory)

        return NotificationStatementAccountAllocator(
            dependencies: dependencies,
            ledger: CoreDataNotificationClaimLedger(
                storageFacade: UserDataStorageFacade.shared,
                txService: durable.txService
            )
        )
    }
}

private extension ServiceCoordinator {
    static func makeNotificationSlotDependencies(
        keyResolver: BandersnatchKeyResolving,
        tldProvider: DotNsTldProviding
    ) -> NotificationSlotDependencies {
        let chainId = AppConfig.Chains.chatChain
        let chainRegistry = ChainRegistryFacade.sharedRegistry
        let storageRequestFactory = makeStorageRequestFactory()

        return NotificationSlotDependencies(
            chainId: chainId,
            sources: NotificationSlotSources(
                repository: NotificationSlotRepository(
                    chainId: chainId,
                    chainRegistry: chainRegistry,
                    storageRequestFactory: storageRequestFactory,
                    resourcesParameters: ResourcesParametersFacade.shared
                ),
                originPersonProvider: ChainOriginPersonProvider(
                    chainId: chainId,
                    chainRegistry: chainRegistry,
                    keyResolver: keyResolver
                ),
                networkSuffixProvider: tldProvider
            ),
            parameters: NotificationParametersProvider(
                chainId: chainId,
                chainRegistry: chainRegistry,
                chainTimeProvider: ChainTimeProvider(
                    chainId: chainId,
                    chainRegistry: chainRegistry,
                    storageRequestFactory: storageRequestFactory
                )
            ),
            issueDiagnostics: NotificationSlotIssueDiagnostics.make(reporter: IssueReportingFacade.shared),
            logger: Logger.shared
        )
    }

    static func registerNotificationSlotDomain(
        in durable: DurableTxServices,
        dependencies: NotificationSlotDependencies,
        originFactory: AsResourcesOriginCreating
    ) {
        let chainId = AppConfig.Chains.chatChain

        durable.policies.register(
            NotificationSlotSubmissionPolicy(
                dependencies: dependencies,
                originFactory: originFactory,
                factory: durable.factory
            ),
            for: NotificationSlotDomain.policyId
        )

        durable.oracles.register(
            NotificationAllowanceOracle.make(
                chainId: chainId,
                allowanceRepository: StatementStoreAllowanceRepository(
                    chainId: chainId,
                    chainRegistry: ChainRegistryFacade.sharedRegistry,
                    storageRequestFactory: makeStorageRequestFactory()
                ),
                logger: Logger.shared
            ),
            for: NotificationSlotDomain.domainId
        )
    }

    static func makeStorageRequestFactory() -> StorageRequestFactoryProtocol {
        StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: OperationManagerFacade.sharedDefaultQueue)
        )
    }
}
