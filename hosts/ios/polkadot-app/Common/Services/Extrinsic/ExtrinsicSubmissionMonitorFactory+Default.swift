import Foundation
import ChainRegistry

extension ExtrinsicSubmissionMonitorFacade {
    static func `default`() -> ExtrinsicSubmissionMonitorFacade {
        ExtrinsicSubmissionMonitorFacade(
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            substrateStorageFacade: SubstrateDataStorageFacade.shared,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            logger: Logger.shared
        )
    }
}
