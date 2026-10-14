import Foundation
import SubstrateSdk
import ExtrinsicService
import SubstrateOperation
import ChainStore
import SDKLogger

public enum ValidatingExtrinsicSubmitterFactory {
    public static func makeResubmittingSubmitter(
        base: ExtrinsicSubmitting,
        chainId: ChainId,
        chainRegistry: ChainResourceProtocol,
        operationQueue: OperationQueue,
        maxAttempts: Int? = nil,
        trackingTill: ExtrinsicTrackingTill = .inBlock,
        logger: SDKLoggerProtocol? = nil
    ) -> ValidatingExtrinsicSubmitter {
        let validationApi = TaggedTransactionQueueApi(
            chainRegistry: chainRegistry,
            operationQueue: operationQueue
        )

        let blockInfoProvider = BlockInfoProvider(
            chainRegistry: chainRegistry,
            operationQueue: operationQueue,
            chainId: chainId
        )

        let recovery = ResubmitWhenValidRecovery(
            chainId: chainId,
            maxAttempts: maxAttempts,
            validationApi: validationApi,
            blockInfoProvider: blockInfoProvider,
            logger: logger
        )

        return ValidatingExtrinsicSubmitter(
            base: base,
            validationApi: validationApi,
            recovery: recovery,
            blockInfoProvider: blockInfoProvider,
            chainId: chainId,
            trackingTill: trackingTill,
            logger: logger
        )
    }
}
