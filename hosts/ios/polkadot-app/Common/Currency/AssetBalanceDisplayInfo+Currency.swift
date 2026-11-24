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

    /// An amount of the asset `assetCode` names, counted in `decimals`: a
    /// supported currency as the app sets a price ("€25.10"), anything else
    /// with its code after the figure ("51.80 USDT").
    static func from(assetCode: String, decimals: Int16) -> AssetBalanceDisplayInfo {
        let code = assetCode.uppercased()
        guard let currency = Currency.supported.first(where: { $0.code == code }) else {
            return AssetBalanceDisplayInfo(
                displayPrecision: UInt16(clamping: min(decimals, 6)),
                assetPrecision: decimals,
                symbol: code,
                symbolValueSeparator: " ",
                symbolPosition: .suffix,
                icon: nil
            )
        }

        let info = from(currency: currency)
        return AssetBalanceDisplayInfo(
            displayPrecision: currency.minorDigits,
            assetPrecision: decimals,
            symbol: info.symbol,
            symbolValueSeparator: info.symbolValueSeparator,
            symbolPosition: info.symbolPosition,
            icon: nil
        )
    }
}
