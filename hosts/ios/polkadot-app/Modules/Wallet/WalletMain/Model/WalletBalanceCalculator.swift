import Foundation
import BigInt
import SubstrateSdk
import AssetsManagement
import ChainRegistry

protocol WalletBalanceCalculatorProtocol {
    func calculateTotalBalance(
        from balances: [ChainAssetId: AssetBalance],
        assets: [ChainAsset],
        prices: [ChainAssetId: PriceData]
    ) -> Decimal

    func calculateTransferableBalance(
        from balances: [ChainAssetId: AssetBalance],
        assets: [ChainAsset],
        prices: [ChainAssetId: PriceData]
    ) -> Decimal
}

final class WalletBalanceCalculator: WalletBalanceCalculatorProtocol {
    private func getPrice(
        from balance: BigUInt,
        displayInfo: AssetBalanceDisplayInfo,
        priceData: PriceData?
    ) -> Decimal {
        guard let price = priceData?.decimalRate else {
            return 0
        }

        let decimalBalance = balance.decimal(assetInfo: displayInfo)

        return decimalBalance * price
    }

    func calculateTotalBalance(
        from balances: [ChainAssetId: AssetBalance],
        assets: [ChainAsset],
        prices: [ChainAssetId: PriceData]
    ) -> Decimal {
        let totalAssets = assets.reduce(Decimal(0)) { price, asset in
            guard let balance = balances[asset.chainAssetId] else {
                return price
            }

            let amountPrice = getPrice(
                from: balance.totalInPlank,
                displayInfo: asset.assetDisplayInfo,
                priceData: prices[balance.chainAssetId]
            )

            return price + amountPrice
        }

        return totalAssets
    }

    func calculateTransferableBalance(
        from balances: [ChainAssetId: AssetBalance],
        assets: [ChainAsset],
        prices: [ChainAssetId: PriceData]
    ) -> Decimal {
        let totalAssets = assets.reduce(Decimal(0)) { price, asset in
            guard let balance = balances[asset.chainAssetId] else {
                return price
            }

            let amountPrice = getPrice(
                from: balance.transferable,
                displayInfo: asset.assetDisplayInfo,
                priceData: prices[balance.chainAssetId]
            )

            return price + amountPrice
        }

        return totalAssets
    }
}
