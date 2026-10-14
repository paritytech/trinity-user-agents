import Foundation
import SubstrateSdk

public protocol AssetExchageUsdtConverting {
    func convertToUsdt(the asset: ChainAssetProtocol, decimalAmount: Decimal) -> Decimal?
    func convertToAssetDecimalFromUsdt(amount: Decimal, asset: ChainAssetProtocol) -> Decimal?
}

extension AssetExchageUsdtConverting {
    func convertToUsdt(the asset: ChainAssetProtocol, amountInPlank: Balance) -> Decimal? {
        let decimalAmount = Decimal.fromSubstrateAmount(
            amountInPlank,
            precision: asset.assetInterface.decimalPrecision
        ) ?? 0

        return convertToUsdt(the: asset, decimalAmount: decimalAmount)
    }

    func convertToAssetInPlankFromUsdt(amount: Decimal, asset: ChainAssetProtocol) -> Balance? {
        let decimalAmount = convertToAssetDecimalFromUsdt(amount: amount, asset: asset)

        return decimalAmount?.toSubstrateAmount(precision: asset.assetInterface.decimalPrecision)
    }
}

public final class AssetExchageUsdtConverter {
    let priceStore: AssetExchangePriceStoring
    let usdtTiedAsset: ChainAssetId

    public init(priceStore: AssetExchangePriceStoring, usdtTiedAsset: ChainAssetId) {
        self.priceStore = priceStore
        self.usdtTiedAsset = usdtTiedAsset
    }
}

extension AssetExchageUsdtConverter: AssetExchageUsdtConverting {
    public func convertToUsdt(the asset: ChainAssetProtocol, decimalAmount: Decimal) -> Decimal? {
        guard
            let usdtPriceRate = priceStore.fetchPrice(for: usdtTiedAsset),
            let assetPriceRate = priceStore.fetchPrice(for: asset.chainAssetId),
            usdtPriceRate > 0 else {
            return nil
        }

        return decimalAmount * assetPriceRate / usdtPriceRate
    }

    public func convertToAssetDecimalFromUsdt(amount: Decimal, asset: ChainAssetProtocol) -> Decimal? {
        guard
            let usdtPriceRate = priceStore.fetchPrice(for: usdtTiedAsset),
            let assetPriceRate = priceStore.fetchPrice(for: asset.chainAssetId),
            assetPriceRate > 0 else {
            return nil
        }

        return amount * usdtPriceRate / assetPriceRate
    }
}
