import Foundation
import Operation_iOS
import SubstrateSdk
import AssetExchange

final class HydraExchangeOperationPrototype: AssetExchangeBaseOperationPrototype {
    let host: HydraExchangeHostProtocol

    init(assetIn: ChainAssetProtocol, assetOut: ChainAssetProtocol, host: HydraExchangeHostProtocol) {
        self.host = host

        super.init(assetIn: assetIn, assetOut: assetOut)
    }
}

extension HydraExchangeOperationPrototype: AssetExchangeOperationPrototypeProtocol {
    func estimatedCostInUsdt(using converter: AssetExchageUsdtConverting) throws -> Decimal {
        let nativeAsset = try assetIn.chainInterface.utilityChainAssetInterfaceOrError()

        return converter.convertToUsdt(the: nativeAsset, decimalAmount: 0.5) ?? 0
    }

    func estimatedExecutionTimeWrapper() -> CompoundOperationWrapper<TimeInterval> {
        host.executionTimeEstimator.totalTimeWrapper(for: [host.chain.chainId])
    }
}
