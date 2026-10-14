import BackgroundExecution
import DurableTransactions
import Foundation

extension ServiceCoordinator {
    /// The one durability layer every domain shares. Built once; a domain registers its oracle and its
    /// policies with the returned registries before it submits anything. The coordinator starts and
    /// stops the head-driven recovery for all of them, so no domain owns the engine.
    static func createDurableTransactionEngine(
        chainViewFactory: any PinnedChainViewFactoryProtocol
    ) -> DurableTxServices {
        let chainRegistry = ChainRegistryFacade.sharedRegistry
        let operationQueue = OperationManagerFacade.sharedDefaultQueue

        let extrinsicFacade = ExtrinsicSubmissionMonitorFacade(
            chainRegistry: chainRegistry,
            substrateStorageFacade: SubstrateDataStorageFacade.shared,
            operationQueue: operationQueue
        )

        return DurableTxService.make(
            store: DurableTxCoreDataRepository(
                storageFacade: UserDataStorageFacade.shared,
                rowObservers: [CoinageTxRowObserver()]
            ),
            chainViewFactory: chainViewFactory,
            chainTools: DurableChainToolsProvider(
                chainRegistry: chainRegistry,
                extrinsicFacade: extrinsicFacade,
                versionProvider: ExtrinsicVersionProvider(),
                signedChains: [AppConfig.Chains.assethubChain]
            ),
            backgroundExecutor: ConnectionRetainingExecutor(provider: chainRegistry),
            logger: Logger.shared
        )
    }
}
