import Foundation
import Operation_iOS
import SubstrateSdk
import ExtrinsicService
import AssetExchange
import SDKLogger
import BigInt

public final class HydraFeeEstimatingFactory {
    let host: ExtrinsicFeeEstimatorHostProtocol
    let feeBufferInPercentage: BigRational
    let assetDetector: HydrationAssetDetecting
    let tokenConverter: HydrationTokenConverting
    let logger: SDKLoggerProtocol

    private var hydraFlowState: HydraFlowState?

    public init(
        host: ExtrinsicFeeEstimatorHostProtocol,
        feeBufferInPercentage: BigRational = BigRational.percent(of: 0), // no overestimation by default
        assetDetector: HydrationAssetDetecting,
        tokenConverter: HydrationTokenConverting,
        logger: SDKLoggerProtocol
    ) {
        self.host = host
        self.feeBufferInPercentage = feeBufferInPercentage
        self.assetDetector = assetDetector
        self.tokenConverter = tokenConverter
        self.logger = logger
    }

    private func setupHydraFlowState() -> HydraFlowState {
        if let hydraFlowState {
            return hydraFlowState
        }

        let hydraFlowState = FeeSharedStateStore.getOrCreateHydra(
            for: host,
            tokenConverter: tokenConverter
        )

        self.hydraFlowState = hydraFlowState

        return hydraFlowState
    }
}

extension HydraFeeEstimatingFactory: ExtrinsicFeeEstimatorProviding {
    public func provideFeeEstimator(for chainAsset: ChainAssetProtocol) -> ExtrinsicFeeEstimating? {
        guard assetDetector.canPayFee(using: chainAsset) else {
            return nil
        }

        let hydraState = setupHydraFlowState()
        let hydraQuoteFactory = HydraQuoteFactory(flowState: hydraState, logger: logger)

        let quoteFactory = HydraFeeQuoteFactory(
            chain: chainAsset.chainInterface,
            realQuoteFactory: hydraQuoteFactory,
            connection: host.connection,
            runtimeService: host.runtimeProvider,
            tokenConverter: tokenConverter,
            operationQueue: host.operationQueue
        )

        return ExtrinsicAssetConversionFeeEstimator(
            chainAsset: chainAsset,
            operationQueue: host.operationQueue,
            quoteFactory: quoteFactory,
            feeBufferInPercentage: feeBufferInPercentage
        )
    }
}
