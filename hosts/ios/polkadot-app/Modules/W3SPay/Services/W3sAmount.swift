import Foundation

struct W3sAmount: Equatable {
    let decimal: Decimal
    /// Always exactly two decimal places, decimal-separator ".", no grouping.
    let normalizedString: String
}

extension W3sAmount {
    /// `Decimal(string:)` alone accepts `9.005`, `1e3`, `+9` — all forbidden by
    /// the W3S spec — so the regex must run before parsing.
    static func parse(_ raw: String, maxUnits: Decimal? = nil) -> W3sAmount? {
        let pattern = #"^[0-9]+(\.[0-9]{1,2})?$"#
        guard raw.range(of: pattern, options: .regularExpression) != nil else { return nil }
        guard let decimal = Decimal(string: raw, locale: Locale(identifier: "en_US_POSIX")) else { return nil }
        if let maxUnits, decimal > maxUnits { return nil }
        return fromValidatedDecimal(decimal)
    }

    static func fromValidatedDecimal(_ value: Decimal) -> W3sAmount? {
        guard value >= 0 else { return nil }
        let formatter = NumberFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.numberStyle = .decimal
        formatter.usesGroupingSeparator = false
        formatter.minimumFractionDigits = 2
        formatter.maximumFractionDigits = 2
        formatter.roundingMode = .halfUp
        guard let string = formatter.string(from: value as NSDecimalNumber) else { return nil }
        return W3sAmount(decimal: value, normalizedString: string)
    }
}
