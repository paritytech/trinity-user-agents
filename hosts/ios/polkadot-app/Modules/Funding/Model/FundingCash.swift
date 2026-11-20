import BigInt
import Foundation
import FoundationExt
import Foundation_iOS
import SubstrateSdk
import TrUAPIHost

/// The payment balance as the funding screens show it: amounts cross the FFI
/// as decimal strings of the asset's smallest unit, and are drawn as the app
/// draws the digital dollar figure, followed by the asset's symbol.
struct FundingCash: Equatable {
    let symbol: String
    let figureInfo: AssetBalanceDisplayInfo

    init(symbol: String, precision: Int16) {
        self.symbol = symbol
        figureInfo = .digitalDollarFigure(assetPrecision: precision)
    }

    /// Nil when `units` is not a number.
    func decimal(_ units: U128) -> Decimal? {
        figureInfo.decimal(units: units)
    }

    func decimal(_ units: U128?) -> Decimal? {
        units.flatMap { decimal($0) }
    }

    func decimal(fromPlanks balance: Balance) -> Decimal? {
        Decimal.fromSubstrateAmount(balance, precision: figureInfo.assetPrecision)
    }

    /// `value` in CASH units, rounded down, so the host never asks for more
    /// than the user has; nil when negative.
    func units(_ value: Decimal) -> U128? {
        guard value >= 0 else { return nil }

        return value.toSubstrateAmount(precision: figureInfo.assetPrecision).map { String($0) }
    }

    /// `text`, a typed amount with `.` as its separator, in CASH units; nil
    /// unless it is digits with at most one `.`.
    func units(text: String) -> U128? {
        guard text.contains(where: \.isASCIIDigit),
              text.allSatisfy({ $0.isASCIIDigit || $0 == "." }),
              text.filter({ $0 == "." }).count <= 1,
              let value = Decimal(string: text, locale: Locale(identifier: "en_US_POSIX"))
        else { return nil }

        return units(value)
    }

    /// "$50.20", with cents only when there are any.
    func figure(_ value: Decimal) -> String {
        figureInfo.amount(value)
    }

    /// "$50.20 CASH".
    func label(_ value: Decimal) -> String {
        "\(figure(value)) \(symbol)"
    }

    /// "$50.20 CASH" for `units`; nil when they are not a number.
    func label(units: U128) -> String? {
        decimal(units).map { label($0) }
    }

    /// "+$50 CASH" for value moving in, "-$50 CASH" for value moving out.
    func signed(_ value: Decimal, direction: FundingDirection) -> String {
        let sign = direction == .in ? "+" : "-"
        return sign + label(value)
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

private extension Character {
    var isASCIIDigit: Bool { isASCII && isNumber }
}
