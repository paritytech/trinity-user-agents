import Foundation
import Operation_iOS
import SubstrateSdk

public protocol AssetExchangeOperationPrototypeProtocol {
    var assetIn: ChainAssetProtocol { get }
    var assetOut: ChainAssetProtocol { get }

    func estimatedCostInUsdt(using converter: AssetExchageUsdtConverting) throws -> Decimal

    func estimatedExecutionTimeWrapper() -> CompoundOperationWrapper<TimeInterval>
}

open class AssetExchangeBaseOperationPrototype {
    public let assetIn: ChainAssetProtocol
    public let assetOut: ChainAssetProtocol

    public init(assetIn: ChainAssetProtocol, assetOut: ChainAssetProtocol) {
        self.assetIn = assetIn
        self.assetOut = assetOut
    }
}
