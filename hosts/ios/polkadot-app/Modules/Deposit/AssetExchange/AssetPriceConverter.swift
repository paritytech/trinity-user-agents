import Foundation
import SubstrateSdk
import AssetExchange
import ChainRegistry

protocol AssetPriceConverting {
    func convert(fiatAmount: Decimal, to chainAssetId: ChainAssetId) throws -> Balance
}

enum AssetPriceConverterError: Error {
    case noPrice(ChainAssetId)
    case brokenTokenAmount(ChainAssetId)
}

final class AssetPriceConverter {
    let chainRegistry: ChainRegistryProtocol
    let priceStore: AssetExchangePriceStoring

    init(chainRegistry: ChainRegistryProtocol, priceStore: AssetExchangePriceStoring) {
        self.chainRegistry = chainRegistry
        self.priceStore = priceStore
    }
}

extension AssetPriceConverter: AssetPriceConverting {
    func convert(fiatAmount: Decimal, to chainAssetId: ChainAssetId) throws -> Balance {
        guard let rate = priceStore.fetchPrice(for: chainAssetId), rate > 0 else {
            throw AssetPriceConverterError.noPrice(chainAssetId)
        }

        let chain = try chainRegistry.getChainOrError(for: chainAssetId.chainId)
        let chainAsset = try chain.chainAssetInterfaceOrError(for: chainAssetId.assetId)

        let assetAmount = fiatAmount / rate

        let precision = chainAsset.assetInterface.decimalPrecision
        return try assetAmount.toSubstrateAmount(precision: precision).mapOrThrow(
            AssetPriceConverterError.brokenTokenAmount(chainAssetId)
        )
    }
}
