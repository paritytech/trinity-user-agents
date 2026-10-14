import Foundation
import SubstrateSdk
import ExtrinsicService

public protocol ExtrinsicFeeEstimatorProviding {
    func provideFeeEstimator(for chainAsset: ChainAssetProtocol) -> ExtrinsicFeeEstimating?
}

public final class ExtrinsicCustomFeeEstimatorFactory {
    let providers: [ExtrinsicFeeEstimatorProviding]

    public init(providers: [ExtrinsicFeeEstimatorProviding]) {
        self.providers = providers
    }
}

extension ExtrinsicCustomFeeEstimatorFactory: ExtrinsicCustomFeeEstimatingFactoryProtocol {
    public func createCustomFeeEstimator(
        for chainAsset: ChainAssetProtocol
    ) -> ExtrinsicFeeEstimating? {
        for provider in providers {
            if let feeEstimator = provider.provideFeeEstimator(for: chainAsset) {
                return feeEstimator
            }
        }

        return nil
    }
}
