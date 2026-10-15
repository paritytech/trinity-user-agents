import Foundation
import Foundation_iOS
import ChainRegistry

extension AssetModel {
    var displayInfo: AssetBalanceDisplayInfo {
        AssetBalanceDisplayInfo(
            displayPrecision: 3,
            assetPrecision: Int16(bitPattern: precision),
            symbol: symbol,
            symbolValueSeparator: " ",
            symbolPosition: .suffix,
            icon: icon
        )
    }

    var digitalDollarDisplayInfo: AssetBalanceDisplayInfo {
        AssetBalanceDisplayInfo(
            displayPrecision: 2,
            assetPrecision: Int16(bitPattern: precision),
            symbol: PaymentAssetSymbol.current,
            symbolValueSeparator: " ",
            symbolPosition: .suffix,
            icon: nil
        )
    }
}

extension AssetModel {
    /// The digital dollar figure alone: fiat symbol in front, no asset symbol after it.
    ///
    /// The asset symbol is set separately and more quietly beside the figure (see `DSAmount`), so
    /// it must not be part of the formatted number.
    var digitalDollarFigureDisplayInfo: AssetBalanceDisplayInfo {
        AssetBalanceDisplayInfo(
            displayPrecision: 2,
            assetPrecision: Int16(bitPattern: precision),
            symbol: AppConfig.Brand.fiatSymbol,
            symbolValueSeparator: "",
            symbolPosition: .prefix,
            icon: nil
        )
    }
}

extension ChainAsset {
    var assetDisplayInfo: AssetBalanceDisplayInfo { asset.displayInfo }
}

extension AssetBalanceDisplayInfo {
    static var usd: AssetBalanceDisplayInfo {
        AssetBalanceDisplayInfo(
            displayPrecision: 2,
            assetPrecision: 2,
            symbol: AppConfig.Brand.fiatSymbol,
            symbolValueSeparator: "",
            symbolPosition: .prefix,
            icon: nil
        )
    }

    var withFiatSymbol: AssetBalanceDisplayInfo {
        AssetBalanceDisplayInfo(
            displayPrecision: displayPrecision,
            assetPrecision: assetPrecision,
            symbol: AppConfig.Brand.fiatSymbol,
            symbolValueSeparator: "",
            symbolPosition: .prefix,
            icon: icon
        )
    }

    var withoutSymbol: AssetBalanceDisplayInfo {
        AssetBalanceDisplayInfo(
            displayPrecision: displayPrecision,
            assetPrecision: assetPrecision,
            symbol: "",
            symbolValueSeparator: symbolValueSeparator,
            symbolPosition: symbolPosition,
            icon: icon
        )
    }
}
