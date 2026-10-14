import Foundation

enum AmountInputResult: Equatable {
    case rate(_ value: Decimal)
    case absolute(_ value: Decimal)

    func getIfAbsolute() -> Decimal? {
        switch self {
        case .rate:
            nil
        case let .absolute(value):
            value
        }
    }

    func absoluteValue(from available: Decimal) -> Decimal {
        switch self {
        case let .rate(value):
            max(value * available, 0.0)
        case let .absolute(value):
            value
        }
    }

    var isMax: Bool {
        switch self {
        case let .rate(value):
            value == 1
        case .absolute:
            false
        }
    }
}
