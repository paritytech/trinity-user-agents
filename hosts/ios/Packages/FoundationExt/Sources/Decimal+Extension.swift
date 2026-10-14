import Foundation

public extension Decimal {
    var hasFraction: Bool {
        var truncated = Decimal()
        var value = self
        NSDecimalRound(&truncated, &value, 0, .down)
        return self != truncated
    }
}
