import Foundation
import Operation_iOS
import SubstrateSdk
import AssetExchange

final class AssetHubExchangeOperationPrototype: AssetExchangeBaseOperationPrototype {
    let host: AssetHubExchangeHostProtocol

    init(assetIn: ChainAssetProtocol, assetOut: ChainAssetProtocol, host: AssetHubExchangeHostProtocol) {
        self.host = host

        super.init(assetIn: assetIn, assetOut: assetOut)
    }
}

extension AssetHubExchangeOperationPrototype: AssetExchangeOperationPrototypeProtocol {
    func estimatedCostInUsdt(using converter: AssetExchageUsdtConverting) throws -> Decimal {
        let nativeAsset = try assetIn.chainInterface.utilityChainAssetInterfaceOrError()

        return converter.convertToUsdt(the: nativeAsset, decimalAmount: 0.015) ?? 0
    }

    func estimatedExecutionTimeWrapper() -> CompoundOperationWrapper<TimeInterval> {
        host.executionTimeEstimator.totalTimeWrapper(for: [host.chain.chainId])
    }
}
