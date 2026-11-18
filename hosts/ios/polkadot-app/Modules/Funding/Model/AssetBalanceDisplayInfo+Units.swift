import BigInt
import Foundation
import FoundationExt
import TrUAPIHost

/// Funding amounts cross the FFI as decimal strings of the asset's smallest
/// unit; they are drawn the way the app draws any other amount.
extension AssetBalanceDisplayInfo {
    /// Nil when `units` is not a number.
    func decimal(units: U128) -> Decimal? {
        BigUInt(units).map { $0.decimal(assetInfo: self) }
    }

    /// "$50.20", "€25.10" or "51.8 USDT"; nil when `units` is not a number.
    func amount(units: U128) -> String? {
        decimal(units: units).map { amount($0) }
    }

    /// A currency as the app sets a price, with its minor digits only when
    /// there are any; any other asset as the app sets a token balance.
    func amount(_ value: Decimal) -> String {
        let factory = AssetBalanceFormatterFactory()
        let formatter = symbolPosition == .prefix
            ? factory.createAssetPriceFormatter(
                for: self,
                minimumFractionDigits: value.hasFraction ? displayPrecision : 0
            )
            : factory.createTokenFormatter(for: self)
        return formatter.value(for: .current).stringFromDecimal(value) ?? ""
    }
}
