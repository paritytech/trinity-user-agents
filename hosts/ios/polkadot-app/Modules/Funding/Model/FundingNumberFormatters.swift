import Foundation

/// The number formatters the funding screens draw amounts with, each built
/// once per configuration: building one is costly and rows redraw often.
/// They are never changed once built, which keeps sharing them across
/// threads safe.
enum FundingNumberFormatters {
    private static let lock = NSLock()
    private nonisolated(unsafe) static var cache: [String: NumberFormatter] = [:]

    /// A decimal figure in US style, with between `minimumDigits` and
    /// `maximumDigits` fraction digits.
    static func decimal(minimumDigits: Int, maximumDigits: Int) -> NumberFormatter {
        formatter(key: "decimal \(minimumDigits) \(maximumDigits)") {
            let formatter = NumberFormatter()
            formatter.numberStyle = .decimal
            formatter.locale = Locale(identifier: "en_US")
            formatter.usesGroupingSeparator = true
            formatter.minimumFractionDigits = minimumDigits
            formatter.maximumFractionDigits = maximumDigits
            return formatter
        }
    }

    /// A `code` currency amount in the user's locale.
    static func currency(code: String) -> NumberFormatter {
        formatter(key: "currency \(code)") {
            let formatter = NumberFormatter()
            formatter.numberStyle = .currency
            formatter.currencyCode = code
            formatter.locale = .autoupdatingCurrent
            return formatter
        }
    }

    private static func formatter(key: String, make: () -> NumberFormatter) -> NumberFormatter {
        lock.withLock {
            if let cached = cache[key] { return cached }

            let formatter = make()
            cache[key] = formatter
            return formatter
        }
    }
}
