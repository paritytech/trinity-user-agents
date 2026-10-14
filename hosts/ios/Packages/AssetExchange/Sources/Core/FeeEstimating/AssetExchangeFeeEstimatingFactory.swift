import Foundation
import SubstrateSdk
import ExtrinsicService

public final class AssetExchangeFeeEstimatingFactory {
    let graphProxy: AssetQuoteFactoryProtocol
    let operationQueue: OperationQueue
    let feeBufferInPercentage: BigRational

    public convenience init(
        graphProxy: AssetQuoteFactoryProtocol,
        operationQueue: OperationQueue,
    ) {
        self.init(
            graphProxy: graphProxy,
            operationQueue: operationQueue,
            feeBufferInPercentage: AssetExchangeFeeConstants.feeBufferInPercentage
        )
    }

    public init(
        graphProxy: AssetQuoteFactoryProtocol,
        operationQueue: OperationQueue,
        feeBufferInPercentage: BigRational
    ) {
        self.graphProxy = graphProxy
        self.operationQueue = operationQueue
        self.feeBufferInPercentage = feeBufferInPercentage
    }
}

extension AssetExchangeFeeEstimatingFactory: ExtrinsicCustomFeeEstimatingFactoryProtocol {
    public func createCustomFeeEstimator(for chainAsset: ChainAssetProtocol) -> ExtrinsicFeeEstimating? {
        ExtrinsicAssetConversionFeeEstimator(
            chainAsset: chainAsset,
            operationQueue: operationQueue,
            quoteFactory: graphProxy,
            feeBufferInPercentage: feeBufferInPercentage
        )
    }
}
