import Foundation
import SubstrateSdk
import AssetExchange
import ChainRegistry

protocol DepositCalculating {
    func calculateMin(for fee: AssetExchangeFee, chainAssetId: ChainAssetId) throws -> Balance
    func calculateMid(chainAssetId: ChainAssetId) throws -> Balance
}

final class DepositCalculator {
    let chainRegistry: ChainRegistryProtocol
    let assetPriceConverter: AssetPriceConverting

    init(chainRegistry: ChainRegistryProtocol, assetPriceConverter: AssetPriceConverting) {
        self.chainRegistry = chainRegistry
        self.assetPriceConverter = assetPriceConverter
    }
}

enum DepositCalculatorError: Error {
    case unitFallbackFailed
}

extension DepositCalculator: DepositCalculating {
    func calculateMin(for fee: AssetExchangeFee, chainAssetId: ChainAssetId) throws -> Balance {
        let chain = try chainRegistry.getChainOrError(for: chainAssetId.chainId)
        let chainAsset = try chain.chainAssetInterfaceOrError(for: chainAssetId.assetId)

        // we are using amount / 2 for BuyExecution and also need some buffer
        return fee.totalFeeInAssetIn(chainAsset) * 3
    }

    func calculateMid(chainAssetId: ChainAssetId) throws -> Balance {
        do {
            return try assetPriceConverter.convert(
                fiatAmount: DepositServiceConstants.midDepositInUsd,
                to: chainAssetId
            )
        } catch {
            let chain = try chainRegistry.getChainOrError(for: chainAssetId.chainId)
            let chainAsset = try chain.chainAssetInterfaceOrError(for: chainAssetId.assetId)

            guard let unitInPlank = Decimal(1).toSubstrateAmount(
                precision: chainAsset.assetInterface.decimalPrecision
            ) else {
                throw DepositCalculatorError.unitFallbackFailed
            }

            return unitInPlank
        }
    }
}
