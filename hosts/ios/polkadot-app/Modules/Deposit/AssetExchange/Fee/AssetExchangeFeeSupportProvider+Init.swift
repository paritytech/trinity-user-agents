import Foundation
import SubstrateSdk
import AssetExchange
import ChainRegistry

extension AssetsExchangeFeeSupportProvider {
    convenience init(
        ahChainId: ChainId,
        hydrationChainId: ChainId,
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.init(
            feeSupportFetchersProvider: AssetExchangeFeeSupportFetchersProvider(
                ahChainId: ahChainId,
                hydrationChainId: hydrationChainId,
                chainRegistry: chainRegistry,
                operationQueue: operationQueue,
                logger: logger
            ),
            operationQueue: operationQueue,
            logger: logger
        )
    }
}
