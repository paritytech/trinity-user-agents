import Foundation
import Operation_iOS
import SubstrateSdk
import ChainStore

public protocol AssetsExchangePathCostEstimating: AnyObject {
    func costEstimationWrapper(
        for path: AssetExchangeGraphPath
    ) -> CompoundOperationWrapper<AssetsExchangePathCost>
}

public struct AssetsExchangePathCost {
    public let amountInAssetIn: Balance
    public let amountInAssetOut: Balance

    public init(amountInAssetIn: Balance, amountInAssetOut: Balance) {
        self.amountInAssetIn = amountInAssetIn
        self.amountInAssetOut = amountInAssetOut
    }

    static var zero: AssetsExchangePathCost {
        AssetsExchangePathCost(amountInAssetIn: 0, amountInAssetOut: 0)
    }
}

public final class AssetsExchangePathCostEstimator {
    let priceStore: AssetExchangePriceStoring
    let chainRegistry: ChainResourceProtocol
    let usdtLocationChainId: ChainId

    public init(
        priceStore: AssetExchangePriceStoring,
        chainRegistry: ChainResourceProtocol,
        usdtLocationChainId: ChainId
    ) {
        self.priceStore = priceStore
        self.chainRegistry = chainRegistry
        self.usdtLocationChainId = usdtLocationChainId
    }
}

extension AssetsExchangePathCostEstimator: AssetsExchangePathCostEstimating {
    public func costEstimationWrapper(
        for path: AssetExchangeGraphPath
    ) -> CompoundOperationWrapper<AssetsExchangePathCost> {
        let operation = ClosureOperation<AssetsExchangePathCost> {
            guard let usdtTiedAsset = self.chainRegistry.getChainInterface(
                for: self.usdtLocationChainId
            )?.chainAssetInterfaceForSymbol("USDT") else {
                return .zero
            }

            let usdtConverter = AssetExchageUsdtConverter(
                priceStore: self.priceStore,
                usdtTiedAsset: usdtTiedAsset.chainAssetId
            )

            let operations = try AssetExchangeOperationPrototypeFactory().createOperationPrototypes(from: path)

            let totalCostInUsdt = try operations.reduce(Decimal(0)) { total, operation in
                let estimatedCostInUsdt = try operation.estimatedCostInUsdt(using: usdtConverter)

                return total + estimatedCostInUsdt
            }

            guard
                let assetIn = operations.first?.assetIn,
                let assetOut = operations.last?.assetOut else {
                return .zero
            }

            let assetInCost = usdtConverter.convertToAssetInPlankFromUsdt(
                amount: totalCostInUsdt,
                asset: assetIn
            ) ?? .zero

            let assetOutCost = usdtConverter.convertToAssetInPlankFromUsdt(
                amount: totalCostInUsdt,
                asset: assetOut
            ) ?? .zero

            return AssetsExchangePathCost(amountInAssetIn: assetInCost, amountInAssetOut: assetOutCost)
        }

        return CompoundOperationWrapper(targetOperation: operation)
    }
}
