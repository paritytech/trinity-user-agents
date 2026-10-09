import Foundation
import ExtrinsicService
import SubstrateSdk
import SubstrateStorageQuery
import AssetExchange
import Operation_iOS
import ChainRegistry

protocol ExtrinsicSubmissionMonitorFacadeProtocol {
    func createMonitorFactory(chain: ChainProtocol) throws -> ExtrinsicSubmitMonitorFactoryProtocol
}

final class ExtrinsicSubmissionMonitorFacade {
    let chainRegistry: ChainRegistryProtocol
    let operationQueue: OperationQueue
    let extrinsicServiceFactory: ExtrinsicServiceCreating
    let logger: LoggerProtocol

    init(
        extrinsicServiceFactory: ExtrinsicServiceCreating,
        chainRegistry: ChainRegistryProtocol,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.extrinsicServiceFactory = extrinsicServiceFactory
        self.operationQueue = operationQueue
        self.logger = logger
    }

    convenience init(
        chainRegistry: ChainRegistryProtocol,
        substrateStorageFacade: StorageFacadeProtocol,
        operationQueue: OperationQueue,
        extrinsicVersion: Extrinsic.FormatVersion = .V5,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.init(
            extrinsicServiceFactory: ExtrinsicServiceFactory(
                chainRegistry: chainRegistry,
                substrateStorageFacade: substrateStorageFacade,
                customFeeEstimator: ExtrinsicCustomFeeEstimatorFactory(providers: []),
                transactionExtensionFactory: CompoundTxExtensionFactory(),
                extrinsicVersion: extrinsicVersion,
                operationQueue: operationQueue,
                logger: logger
            ),
            chainRegistry: chainRegistry,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}

extension ExtrinsicSubmissionMonitorFacade: ExtrinsicSubmissionMonitorFacadeProtocol {
    func createMonitorFactory(chain: ChainProtocol) throws -> ExtrinsicSubmitMonitorFactoryProtocol {
        try createMonitorFactory(chain: chain, submitter: nil)
    }

    /// The operation factory that builds (and signs) extrinsics up-front — used where the caller
    /// needs the built model before submission, e.g. Coinage's durability tracker.
    func createOperationFactory(chain: ChainProtocol) throws -> ExtrinsicOperationFactoryProtocol {
        try extrinsicServiceFactory.createOperationFactory(chain: chain)
    }

    /// Same, pinned to a format the caller resolved from the runtime (see `ExtrinsicVersionProvider`).
    func createOperationFactory(
        chain: ChainProtocol,
        extrinsicVersion: Extrinsic.Version
    ) throws -> ExtrinsicOperationFactoryProtocol {
        try extrinsicServiceFactory.createOperationFactory(chain: chain, extrinsicVersion: extrinsicVersion)
    }

    /// The fork-protected submitter (pre-submission validation + resubmit-on-fork). `trackingTill`
    /// controls how far the watch follows: `.inBlock` completes on inclusion, `.finalized` keeps
    /// tracking until the block is finalized (used by Coinage's durability tracker).
    func makeForkProtectedSubmitter(
        chain: ChainProtocol,
        trackingTill: ExtrinsicTrackingTill = .inBlock
    ) throws -> ExtrinsicSubmitting {
        try extrinsicServiceFactory.makeForkProtectedSubmitter(chain: chain, trackingTill: trackingTill)
    }

    func makeForkProtectedSubmitter(
        chain: ChainProtocol,
        trackingTill: ExtrinsicTrackingTill,
        extrinsicVersion: Extrinsic.Version
    ) throws -> ExtrinsicSubmitting {
        try extrinsicServiceFactory.makeForkProtectedSubmitter(
            chain: chain,
            trackingTill: trackingTill,
            extrinsicVersion: extrinsicVersion
        )
    }
}

extension ExtrinsicSubmissionMonitorFacade {
    func createMonitorFactory(
        chain: ChainProtocol,
        submitter: ExtrinsicSubmitting?
    ) throws -> ExtrinsicSubmitMonitorFactoryProtocol {
        let connection = try chainRegistry.getConnectionOrError(for: chain.chainId)
        let runtimeProvider = try chainRegistry.getRuntimeProviderOrError(for: chain.chainId)

        let extrinsicService = try extrinsicServiceFactory.createExtrinsicService(
            chain: chain,
            submitter: submitter
        )

        let statusService = ExtrinsicStatusService(
            connection: connection,
            runtimeProvider: runtimeProvider,
            eventsQueryFactory: BlockEventsQueryFactory(
                operationQueue: operationQueue,
                eventsRepository: SubstrateEventsRepository(),
                storageRequestFactory: StorageRequestFactory(
                    remoteFactory: StorageKeyFactory(),
                    operationManager: OperationManager(operationQueue: operationQueue)
                ),
                logger: logger
            )
        )

        return ExtrinsicSubmissionMonitorFactory(
            submissionService: extrinsicService,
            statusService: statusService,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}
