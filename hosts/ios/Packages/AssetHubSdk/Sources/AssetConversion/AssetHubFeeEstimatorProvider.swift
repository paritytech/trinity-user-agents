import Foundation
import SubstrateSdk
import ExtrinsicService
import BigInt
import AssetExchange
import SDKLogger

public final class AssetHubFeeEstimatorProvider {
    let host: ExtrinsicFeeEstimatorHostProtocol
    let feeBufferInPercentage: BigRational
    let feeReporter: AssetHubFeeReporting
    let tokenConverter: AssetHubTokenConverting
    let tokensBulkMapperFactory: AssetHubBulkTokensMapperFactoryProtocol
    let logger: SDKLoggerProtocol

    public init(
        host: ExtrinsicFeeEstimatorHostProtocol,
        feeBufferInPercentage: BigRational = BigRational.percent(of: 0), // no overestimation by default
        tokenConverter: AssetHubTokenConverting,
        tokensBulkMapperFactory: AssetHubBulkTokensMapperFactoryProtocol,
        feeReporter: AssetHubFeeReporting,
        logger: SDKLoggerProtocol
    ) {
        self.host = host
        self.feeBufferInPercentage = feeBufferInPercentage
        self.feeReporter = feeReporter
        self.tokensBulkMapperFactory = tokensBulkMapperFactory
        self.tokenConverter = tokenConverter
        self.logger = logger
    }
}

extension AssetHubFeeEstimatorProvider: ExtrinsicFeeEstimatorProviding {
    public func provideFeeEstimator(for chainAsset: ChainAssetProtocol) -> ExtrinsicFeeEstimating? {
        guard feeReporter.canPayFee(using: chainAsset) else {
            return nil
        }

        let assetHubQuoteFactory = AssetHubSwapOperationFactory(
            chain: host.chain,
            runtimeService: host.runtimeProvider,
            connection: host.connection,
            tokenConverter: tokenConverter,
            bulkMapperFactory: tokensBulkMapperFactory,
            operationQueue: host.operationQueue
        )

        return ExtrinsicAssetConversionFeeEstimator(
            chainAsset: chainAsset,
            operationQueue: host.operationQueue,
            quoteFactory: assetHubQuoteFactory,
            feeBufferInPercentage: feeBufferInPercentage
        )
    }
}
