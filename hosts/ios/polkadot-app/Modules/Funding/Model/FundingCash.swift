import Foundation
import TrUAPIHost

/// The payment balance as the funding screens show it: amounts cross the FFI
/// as decimal strings of the asset's smallest unit, and are drawn as dollars
/// followed by the asset's symbol.
struct FundingCash: Equatable {
    let symbol: String
    let precision: Int16

    func decimal(_ units: U128) -> Decimal {
        guard let value = Decimal(string: units) else { return 0 }

        return value / Self.scale(precision)
    }

    func decimal(_ units: U128?) -> Decimal? {
        units.map { decimal($0) }
    }

    /// Rounds down, so the host never asks for more than the user typed.
    func units(_ value: Decimal) -> U128 {
        var scaled = value * Self.scale(precision)
        var rounded = Decimal()
        NSDecimalRound(&rounded, &scaled, 0, .down)
        return NSDecimalNumber(decimal: max(rounded, 0)).stringValue
    }

    /// "$50.20", with cents only when there are any.
    func figure(_ value: Decimal) -> String {
        "$" + Self.figureFormatter(for: value).string(from: NSDecimalNumber(decimal: value)).orEmpty
    }

    /// "$50.20 CASH".
    func label(_ value: Decimal) -> String {
        "\(figure(value)) \(symbol)"
    }

    /// "+$50 CASH" for value moving in, "-$50 CASH" for value moving out.
    func signed(_ value: Decimal, direction: FundingDirection) -> String {
        let sign = direction == .in ? "+" : "-"
        return sign + label(value)
    }

    static func scale(_ decimals: Int16) -> Decimal {
        pow(Decimal(10), Int(max(decimals, 0)))
    }
}

private extension FundingCash {
    static func figureFormatter(for value: Decimal) -> NumberFormatter {
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        formatter.locale = Locale(identifier: "en_US")
        formatter.usesGroupingSeparator = true
        let hasCents = value != value.fundingRounded(scale: 0)
        formatter.minimumFractionDigits = hasCents ? 2 : 0
        formatter.maximumFractionDigits = 2
        return formatter
    }
}

extension Decimal {
    func fundingRounded(scale: Int, mode: NSDecimalNumber.RoundingMode = .plain) -> Decimal {
        var source = self
        var result = Decimal()
        NSDecimalRound(&result, &source, scale, mode)
        return result
    }
}

private extension String? {
    var orEmpty: String { self ?? "" }
}
