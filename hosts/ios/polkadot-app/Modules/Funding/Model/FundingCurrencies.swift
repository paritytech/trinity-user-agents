import Foundation
import TrUAPIHost

/// What a quote's `send_amount` (In) or `receive_amount` (Out) is counted in:
/// the asset the ask named, in its smallest unit.
///
/// Fiat codes follow ISO 4217, whose minor units Foundation already knows. A
/// crypto symbol has no such table, so the stablecoins and native assets the
/// funding providers name are listed here.
struct FundingAssetUnit: Equatable {
    let code: String
    let decimals: Int16
    let isFiat: Bool

    init(code: String) {
        let upper = code.uppercased()
        self.code = upper

        if let decimals = Self.cryptoDecimals[upper] {
            self.decimals = decimals
            isFiat = false
        } else if Self.isoCurrencyCodes.contains(upper) {
            decimals = Self.fiatDecimals(upper)
            isFiat = true
        } else {
            decimals = Self.defaultCryptoDecimals
            isFiat = false
        }
    }

    init(code: String, decimals: UInt8) {
        let upper = code.uppercased()
        self.code = upper
        self.decimals = Int16(decimals)
        isFiat = Self.isoCurrencyCodes.contains(upper) && Self.cryptoDecimals[upper] == nil
    }

    func decimal(_ units: U128) -> Decimal {
        guard let value = Decimal(string: units) else { return 0 }

        return value / FundingCash.scale(decimals)
    }

    /// "€50.05" for fiat, "51.80 USDT" for crypto.
    func format(_ units: U128) -> String {
        format(decimal: decimal(units))
    }

    func format(decimal value: Decimal) -> String {
        if isFiat {
            let formatter = NumberFormatter()
            formatter.numberStyle = .currency
            formatter.currencyCode = code
            formatter.locale = .current
            return formatter.string(from: NSDecimalNumber(decimal: value)) ?? "\(value) \(code)"
        }

        return "\(Self.plain(value, maximumDigits: Int(min(decimals, 6)))) \(code)"
    }

    /// "25.10 EUR", the way the provider list sets a quote.
    func formatWithCode(_ units: U128) -> String {
        "\(Self.plain(decimal(units), maximumDigits: isFiat ? Int(decimals) : 6, minimumDigits: 2)) \(code)"
    }
}

private extension FundingAssetUnit {
    static let defaultCryptoDecimals: Int16 = 6

    static let cryptoDecimals: [String: Int16] = [
        "USDC": 6,
        "USDT": 6,
        "DOT": 10,
        "ETH": 18,
        "BTC": 8,
        "SOL": 9,
        "TRX": 6,
        "DAI": 18
    ]

    static let isoCurrencyCodes = Set(Locale.Currency.isoCurrencies.map(\.identifier))

    static func fiatDecimals(_ code: String) -> Int16 {
        let formatter = NumberFormatter()
        formatter.numberStyle = .currency
        formatter.currencyCode = code
        return Int16(formatter.maximumFractionDigits)
    }

    static func plain(_ value: Decimal, maximumDigits: Int, minimumDigits: Int = 0) -> String {
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        formatter.locale = Locale(identifier: "en_US")
        formatter.minimumFractionDigits = min(minimumDigits, maximumDigits)
        formatter.maximumFractionDigits = maximumDigits
        return formatter.string(from: NSDecimalNumber(decimal: value)) ?? "\(value)"
    }
}
