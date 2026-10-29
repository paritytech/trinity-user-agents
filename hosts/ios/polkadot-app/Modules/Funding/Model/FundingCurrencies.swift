import Foundation
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
    static let isoCurrencyCodes = Set(Locale.Currency.isoCurrencies.map(\.identifier))

    static func plain(_ value: Decimal, maximumDigits: Int, minimumDigits: Int = 0) -> String {
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        formatter.locale = Locale(identifier: "en_US")
        formatter.minimumFractionDigits = min(minimumDigits, maximumDigits)
        formatter.maximumFractionDigits = maximumDigits
        return formatter.string(from: NSDecimalNumber(decimal: value)) ?? "\(value)"
    }
}
