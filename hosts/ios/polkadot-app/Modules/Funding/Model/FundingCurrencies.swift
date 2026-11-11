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
            return withSign(value)
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
        format(value, symbol: code, separator: " ", position: .suffix, digits: (minimumDigits, maximumDigits))
    }

    /// "€50.05": the currency's sign in the user's locale, and its minor unit.
    func withSign(_ value: Decimal) -> String {
        let sign = (Locale.current as NSLocale).displayName(forKey: .currencySymbol, value: code) ?? code
        let minor = Self.minorDigits(of: code)
        return format(value, symbol: sign, separator: "", position: .prefix, digits: (minor, minor))
    }

    func format(
        _ value: Decimal,
        symbol: String,
        separator: String,
        position: TokenSymbolPosition,
        digits: (minimum: Int16, maximum: Int16)
    ) -> String {
        let info = AssetBalanceDisplayInfo(
            displayPrecision: UInt16(max(digits.maximum, 0)),
            assetPrecision: decimals,
            symbol: symbol,
            symbolValueSeparator: separator,
            symbolPosition: position,
            icon: nil
        )
        let minimum = UInt16(max(min(digits.minimum, digits.maximum), 0))
        return AssetBalanceFormatterFactory()
            .createExactFormatter(for: info, minimumFractionDigits: minimum)
            .value(for: .current)
            .stringFromDecimal(value) ?? "\(value) \(code)"
    }

    /// The digits after the separator `code`'s amounts are written with: 2
    /// for EUR, 0 for JPY.
    static func minorDigits(of code: String) -> Int16 {
        if wholeUnitCodes.contains(code) { return 0 }
        if thousandthCodes.contains(code) { return 3 }
        return 2
    }

    /// ISO 4217 currencies written without a minor unit.
    static let wholeUnitCodes: Set<String> = [
        "BIF", "CLP", "DJF", "GNF", "ISK", "JPY", "KMF", "KRW", "PYG",
        "RWF", "UGX", "UYI", "VND", "VUV", "XAF", "XOF", "XPF"
    ]

    /// ISO 4217 currencies written with three digits after the separator.
    static let thousandthCodes: Set<String> = ["BHD", "IQD", "JOD", "KWD", "LYD", "OMR", "TND"]
}
