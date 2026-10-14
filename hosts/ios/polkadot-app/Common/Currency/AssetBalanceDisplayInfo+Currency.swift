import Foundation

extension AssetBalanceDisplayInfo {
    static func from(currency: Currency) -> AssetBalanceDisplayInfo {
        AssetBalanceDisplayInfo(
            displayPrecision: 2,
            assetPrecision: 2,
            symbol: currency.symbol ?? currency.code,
            symbolValueSeparator: (currency.symbol != nil) ? "" : " ",
            symbolPosition: (currency.symbol != nil) ? .prefix : .suffix,
            icon: nil
        )
    }
}
