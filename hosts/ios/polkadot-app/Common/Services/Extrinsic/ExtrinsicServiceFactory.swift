import Foundation
import Operation_iOS
import Keystore_iOS
import SubstrateSdk
import SubstrateSdkExt
import SubstrateMetadataHash
import ExtrinsicService
import ExtrinsicServiceExt
import ChainRegistry

protocol ExtrinsicServiceCreating: ExtrinsicServiceFactoryProtocol {
    func createExtrinsicService(
        chain: ChainProtocol,
        submitter: ExtrinsicSubmitting?
    ) throws -> ExtrinsicServiceProtocol

    func createOperationFactory(chain: ChainProtocol) throws -> ExtrinsicOperationFactoryProtocol

    /// An operation factory pinned to `extrinsicVersion` instead of the factory's default format — for
    /// callers that resolve the format from the runtime (see `ExtrinsicVersionProvider`).
    func createOperationFactory(
        chain: ChainProtocol,
        extrinsicVersion: Extrinsic.Version
    ) throws -> ExtrinsicOperationFactoryProtocol

    func makeForkProtectedSubmitter(
        chain: ChainProtocol,
        trackingTill: ExtrinsicTrackingTill
    ) throws -> ExtrinsicSubmitting

    func makeForkProtectedSubmitter(
        chain: ChainProtocol,
        trackingTill: ExtrinsicTrackingTill,
        extrinsicVersion: Extrinsic.Version
    ) throws -> ExtrinsicSubmitting
}

final class ExtrinsicServiceFactory {
    private let chainRegistry: ChainRegistryProtocol
    private let operationQueue: OperationQueue
    private let metadataHashOperationFactory: MetadataHashOperationFactoryProtocol
    private let customFeeEstimator: ExtrinsicCustomFeeEstimatingFactoryProtocol
    private let transactionExtensionFactory: ExtrinsicTransactionExtensionMaking
    private let extrinsicVersion: Extrinsic.FormatVersion
    private let extensionVersionProvider: ExtrinsicExtensionVersionProviding
    private let logger: LoggerProtocol

    init(
        chainRegistry: ChainRegistryProtocol,
        substrateStorageFacade: StorageFacadeProtocol,
        customFeeEstimator: ExtrinsicCustomFeeEstimatingFactoryProtocol,
        transactionExtensionFactory: ExtrinsicTransactionExtensionMaking,
        extrinsicVersion: Extrinsic.FormatVersion = .V5,
        extensionVersionProvider: ExtrinsicExtensionVersionProviding = ExtrinsicExtensionVersionProvider(),
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.chainRegistry = chainRegistry

        let metadataItemProvider = RuntimeMetadataItemProvider(
            runtimeMetadataRepositoryFactory: RuntimeMetadataRepositoryFactory(storageFacade: substrateStorageFacade)
        )

        metadataHashOperationFactory = MetadataHashOperationFactory(
            metadataItemProvider: metadataItemProvider,
            operationQueue: operationQueue
        )

        self.operationQueue = operationQueue
        self.extrinsicVersion = extrinsicVersion
        self.extensionVersionProvider = extensionVersionProvider
        self.customFeeEstimator = customFeeEstimator
        self.transactionExtensionFactory = transactionExtensionFactory
        self.logger = logger
    }
}

extension ExtrinsicServiceFactory: ExtrinsicServiceCreating {
    func createExtrinsicService(chain: ChainProtocol) throws -> ExtrinsicServiceProtocol {
        try createExtrinsicService(chain: chain, submitter: nil)
    }

    func createExtrinsicService(
        chain: ChainProtocol,
        submitter: ExtrinsicSubmitting?
    ) throws -> ExtrinsicServiceProtocol {
        let connection = try chainRegistry.getConnectionOrError(for: chain.chainId)
        let runtimeProvider = try chainRegistry.getRuntimeProviderOrError(for: chain.chainId)
        let chainModel = try chainRegistry.getChainOrError(for: chain.chainId)
        let extrinsicVersion = resolveExtrinsicVersion(for: chain)

        Logger.shared.info("Extrinsic version: \(extrinsicVersion)")

        let host = ExtrinsicFeeEstimatorHost(
            chain: chain,
            connection: connection,
            runtimeProvider: runtimeProvider,
            operationQueue: operationQueue,
            logger: logger
        )

        let feeEstimatingFactory = ExtrinsicFeeEstimatingWrapperFactory(
            host: host,
            customFeeEstimatorFactory: customFeeEstimator
        )

        return try ExtrinsicService(
            chain: chain,
            extrinsicVersion: extrinsicVersion,
            runtimeRegistry: runtimeProvider,
            feeEstimationRegistry: ExtrinsicFeeEstimationRegistry(
                chain: chain,
                estimatingWrapperFactory: feeEstimatingFactory,
                feeInstallingWrapperFactory: ExtrinsicFeeInstallingFactory(host: host)
            ),
            metadataHashOperationFactory: metadataHashOperationFactory,
            eraOperationFactory: MortalEraOperationFactory(chain: chainModel),
            extensions: transactionExtensionFactory.createExtensions(),
            engine: connection,
            operationQueue: operationQueue,
            timeout: JSONRPCTimeout.hour,
            submitter: submitter ?? makeForkProtectedSubmitter(chain: chain)
        )
    }

    func createOperationFactory(chain: ChainProtocol) throws -> ExtrinsicOperationFactoryProtocol {
        try createOperationFactory(chain: chain, extrinsicVersion: resolveExtrinsicVersion(for: chain))
    }

    func createOperationFactory(
        chain: ChainProtocol,
        extrinsicVersion: Extrinsic.Version
    ) throws -> ExtrinsicOperationFactoryProtocol {
        let connection = try chainRegistry.getConnectionOrError(for: chain.chainId)
        let runtimeProvider = try chainRegistry.getRuntimeProviderOrError(for: chain.chainId)
        let chainModel = try chainRegistry.getChainOrError(for: chain.chainId)

        let host = ExtrinsicFeeEstimatorHost(
            chain: chain,
            connection: connection,
            runtimeProvider: runtimeProvider,
            operationQueue: operationQueue,
            logger: logger
        )

        let feeEstimatingFactory = ExtrinsicFeeEstimatingWrapperFactory(
            host: host,
            customFeeEstimatorFactory: customFeeEstimator
        )

        return ExtrinsicOperationFactory(
            chain: chain,
            extrinsicVersion: extrinsicVersion,
            feeEstimationRegistry: ExtrinsicFeeEstimationRegistry(
                chain: chain,
                estimatingWrapperFactory: feeEstimatingFactory,
                feeInstallingWrapperFactory: ExtrinsicFeeInstallingFactory(host: host)
            ),
            runtimeRegistry: runtimeProvider,
            customExtensions: transactionExtensionFactory.createExtensions(),
            engine: connection,
            metadataHashOperationFactory: metadataHashOperationFactory,
            eraOperationFactory: MortalEraOperationFactory(chain: chainModel),
            operationQueue: operationQueue,
            timeout: JSONRPCTimeout.singleNode
        )
    }
}

extension ExtrinsicServiceFactory {
    /// Resolves the concrete version for the configured format (default V5). The V5 extension version
    /// is sourced per-chain from remote config (default 0); the format is never flipped to V4 unless
    /// the caller pinned it.
    func resolveExtrinsicVersion(for chain: ChainProtocol) -> Extrinsic.Version {
        extensionVersionProvider.getExtensionVersion(for: extrinsicVersion, chainId: chain.chainId)
    }

    func makeForkProtectedSubmitter(
        chain: ChainProtocol,
        trackingTill: ExtrinsicTrackingTill = .inBlock
    ) throws -> ExtrinsicSubmitting {
        try makeForkProtectedSubmitter(
            chain: chain,
            trackingTill: trackingTill,
            extrinsicVersion: resolveExtrinsicVersion(for: chain)
        )
    }

    func makeForkProtectedSubmitter(
        chain: ChainProtocol,
        trackingTill: ExtrinsicTrackingTill,
        extrinsicVersion: Extrinsic.Version
    ) throws -> ExtrinsicSubmitting {
        let base = try DefaultExtrinsicSubmitter(
            operationFactory: createOperationFactory(chain: chain, extrinsicVersion: extrinsicVersion),
            operationQueue: operationQueue,
            timeout: JSONRPCTimeout.hour
        )

        return ValidatingExtrinsicSubmitterFactory.makeResubmittingSubmitter(
            base: base,
            chainId: chain.chainId,
            chainRegistry: chainRegistry,
            operationQueue: operationQueue,
            maxAttempts: 10,
            trackingTill: trackingTill,
            logger: logger
        )
    }
}
