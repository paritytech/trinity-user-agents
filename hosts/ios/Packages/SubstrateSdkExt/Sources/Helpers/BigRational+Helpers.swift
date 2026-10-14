import Foundation
import SubstrateSdk
import BigInt

public extension BigRational {
    /// A whole, i.e. 100%.
    static var full: BigRational { .percent(of: 100) }

    static func withInt(_ value: some BinaryInteger) -> BigRational {
        BigRational(numerator: BigUInt(value), denominator: 1)
    }
}

extension BigRational: @retroactive Comparable {
    public static func < (lhs: BigRational, rhs: BigRational) -> Bool {
        lhs.numerator * rhs.denominator < rhs.numerator * lhs.denominator
    }

    public static func == (lhs: BigRational, rhs: BigRational) -> Bool {
        lhs.numerator * rhs.denominator == rhs.numerator * lhs.denominator
    }
}
