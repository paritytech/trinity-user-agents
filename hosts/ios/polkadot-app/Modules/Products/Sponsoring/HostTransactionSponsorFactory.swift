import AssetsManagement
import Foundation
import Individuality
import KeyDerivation
import Products
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery
import SubstrateOperation
import ChainRegistry

final class HostTransactionSponsorFactory: TransactionSponsorMaking {
    private let accountManager: ProductsAccountManaging
    private let resourceKeyManager: ProductResourceKeyManaging
    private let chainRegistry: ChainRegistryProtocol
    private let keyResolver: BandersnatchKeyResolving
    private let logger: LoggerProtocol

    init(
        accountManager: ProductsAccountManaging,
        resourceKeyManager: ProductResourceKeyManaging,
        chainRegistry: ChainRegistryProtocol,
        keyResolver: BandersnatchKeyResolving,
        logger: LoggerProtocol
    ) {
        self.accountManager = accountManager
        self.resourceKeyManager = resourceKeyManager
        self.chainRegistry = chainRegistry
        self.keyResolver = keyResolver
        self.logger = logger
    }

    func makePreimageSponsor() -> PreimageSubmitSponsoring {
        let operationQueue = OperationManagerFacade.sharedDefaultQueue

        let storageRequestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )

        let peopleTimeProvider = ChainTimeProvider(
            chainId: AppConfig.Chains.usernameChain,
            chainRegistry: chainRegistry,
            storageRequestFactory: storageRequestFactory
        )

        let bulletInInfoProvider = BulletInSlotInfoProvider(
            bulletInChainId: AppConfig.Chains.bulletInChain,
            peopleChainId: AppConfig.Chains.usernameChain,
            chainRegistry: chainRegistry,
            keyResolver: keyResolver,
            operationQueue: operationQueue,
            chainTimeProvider: peopleTimeProvider,
            resourcesParameters: ResourcesParametersFacade.shared,
            networkSuffixProvider: DotNsTldProviderFacade.shared
        )

        return PreimageSubmitSponsor(
            accountManager: accountManager,
            resourceKeyManager: resourceKeyManager,
            bulletInInfoProvider: bulletInInfoProvider
        )
    }

    func makePGasSponsor() -> PGasTransactionSponsoring {
        let operationQueue = OperationManagerFacade.sharedDefaultQueue

        return PGasTransactionSponsor(
            pgasChainAssetId: AppConfig.Assets.pgasAsset,
            assetQueryTypeMaker: AssetQueryTypeFactory(),
            balanceService: BalanceQueryService(
                chainResource: chainRegistry,
                operationQueue: operationQueue
            ),
            accountManager: accountManager,
            chainResource: chainRegistry,
            operationQueue: operationQueue,
            logger: logger
        )
    }

    func makeStatementStoreSponsor() -> StatementStoreSponsoring {
        let operationQueue = OperationManagerFacade.sharedDefaultQueue

        let storageRequestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )

        let timeProvider = ChainTimeProvider(
            chainId: AppConfig.Chains.chatChain,
            chainRegistry: chainRegistry,
            storageRequestFactory: storageRequestFactory
        )

        let allowanceRepository = AllowanceRepositoryFactory(storageFacade: UserDataStorageFacade.shared)
            .createStatementStoreRepository()
        let accounting = StatementStoreSlotAccountant(repository: allowanceRepository)

        let originPersonProvider = ChainOriginPersonProvider(
            chainId: AppConfig.Chains.chatChain,
            chainRegistry: chainRegistry,
            keyResolver: keyResolver
        )

        let slotInfoProvider = StatementStoreSlotInfoProvider(
            chainId: AppConfig.Chains.chatChain,
            chainRegistry: chainRegistry,
            storageRequestFactory: storageRequestFactory,
            networkSuffixProvider: DotNsTldProviderFacade.shared,
            resourcesParameters: ResourcesParametersFacade.shared,
            chainTimeProvider: timeProvider,
            originPersonProvider: originPersonProvider,
            accounting: accounting,
            logger: logger
        )

        return StatementStoreSponsor(
            accountManager: accountManager,
            resourceKeyManager: resourceKeyManager,
            slotInfoProvider: slotInfoProvider
        )
    }
}
