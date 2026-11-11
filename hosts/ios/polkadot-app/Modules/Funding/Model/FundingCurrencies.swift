import BigInt
import Foundation
import Foundation_iOS
import TrUAPIHost

/// What a quote's `send_amount` (In) or `receive_amount` (Out) is counted in:
/// the asset the ask named, in its smallest unit.
///
/// The decimals come from the quote or deposit that names the asset; an ISO
/// 4217 code is set as currency, anything else as a crypto amount.
struct FundingAssetUnit: Equatable {
    let code: String
    let decimals: Int16
    let isFiat: Bool

    init(code: String, decimals: UInt8) {
        let upper = code.uppercased()
        self.code = upper
        self.decimals = Int16(decimals)
        isFiat = Self.isoCurrencyCodes.contains(upper)
    }

    /// Nil when `units` is not a number.
    func decimal(_ units: U128) -> Decimal? {
        BigUInt(units).flatMap { Decimal.fromSubstrateAmount($0, precision: decimals) }
    }

    /// "€50.05" for fiat, "51.80 USDT" for crypto; nil when `units` is not a
    /// number.
    func format(_ units: U128) -> String? {
        decimal(units).map { format(decimal: $0) }
    }

    func format(decimal value: Decimal) -> String {
        if isFiat {
            let formatter = FundingNumberFormatters.currency(code: code)
            return formatter.string(from: NSDecimalNumber(decimal: value)) ?? "\(value) \(code)"
        }

        return withCode(value, maximumDigits: min(decimals, 6))
    }

    /// "25.10 EUR", the way the provider list sets a quote; nil when `units`
    /// is not a number.
    func formatWithCode(_ units: U128) -> String? {
        decimal(units).map { value in
            withCode(value, maximumDigits: isFiat ? decimals : 6, minimumDigits: 2)
        }
    }
}

private extension FundingAssetUnit {
    static let isoCurrencyCodes = Set(Locale.Currency.isoCurrencies.map(\.identifier))

    func withCode(_ value: Decimal, maximumDigits: Int16, minimumDigits: Int16 = 0) -> String {
        let info = AssetBalanceDisplayInfo(
            displayPrecision: UInt16(max(maximumDigits, 0)),
            assetPrecision: decimals,
            symbol: code,
            symbolValueSeparator: " ",
            symbolPosition: .suffix,
            icon: nil
        )
        return AssetBalanceFormatterFactory()
            .createExactFormatter(for: info, minimumFractionDigits: UInt16(max(min(minimumDigits, maximumDigits), 0)))
            .value(for: .current)
            .stringFromDecimal(value) ?? "\(value) \(code)"
    }
}
