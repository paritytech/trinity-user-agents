import Foundation
import SubstrateSdk
import SubstrateStateCall
import Operation_iOS
import ChainStore
import SubstrateOperation

final class XcmOneOfCallDerivator {
    let chainRegistry: ChainResourceProtocol
    let operationQueue: OperationQueue

    private let featuresFactory = XcmTransferFeaturesFactory()

    init(chainRegistry: ChainResourceProtocol, operationQueue: OperationQueue) {
        self.chainRegistry = chainRegistry
        self.operationQueue = operationQueue
    }
}

private extension XcmOneOfCallDerivator {
    func createCallDerivationWrapper(
        for transferRequest: XcmUnweightedTransferRequest
    ) -> CompoundOperationWrapper<RuntimeCallCollecting> {
        let features = featuresFactory.createFeatures(for: transferRequest.metadata)

        let actualDerivator: XcmCallDerivating =
            if features.shouldUseXcmExecute {
                XcmExecuteDerivator(
                    chainRegistry: chainRegistry,
                    xcmPaymentFactory: XcmPaymentOperationFactory(
                        chainRegistry: chainRegistry,
                        operationQueue: operationQueue
                    ),
                    metadataFactory: XcmPalletMetadataQueryFactory()
                )
            } else {
                XcmTypeBasedCallDerivator(chainRegistry: chainRegistry)
            }

        return actualDerivator.createTransferCallDerivationWrapper(for: transferRequest)
    }
}

extension XcmOneOfCallDerivator: XcmCallDerivating {
    func createTransferCallDerivationWrapper(
        for transferRequest: XcmUnweightedTransferRequest
    ) -> CompoundOperationWrapper<RuntimeCallCollecting> {
        createCallDerivationWrapper(for: transferRequest)
    }
}
