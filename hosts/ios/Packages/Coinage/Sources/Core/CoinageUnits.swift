import BigInt
import Foundation
import SubstrateSdk

/// The payment asset's planks in one cent, the unit the core counts value in.
public struct CoinageUnits: Equatable, Sendable {
    /// Planks in one cent.
    public let unit: BigUInt
    /// The payment asset's decimal precision.
    public let precision: Int16

    public init(unit: BigUInt, precision: Int16) {
        self.unit = unit
        self.precision = precision
    }

    public func planks(ofCents cents: UInt64) -> BigUInt {
        BigUInt(cents) * unit
    }

    /// The value of one coin of `exponent`, `unit · 2^exponent`.
    public func planks(ofExponent exponent: Int16) -> BigUInt {
        unit << Int(exponent)
    }

    /// Whole cents of `planks`, rounded down: the core moves no fraction of a cent.
    public func cents(ofPlanks planks: BigUInt) -> UInt64 {
        guard unit > 0 else { return 0 }
        return UInt64(clamping: planks / unit)
    }

    public func decimal(fromPlanks planks: BigUInt) -> Decimal {
        Decimal.fromSubstrateAmount(planks, precision: precision) ?? 0
    }

    /// The decimal value of one coin of `exponent`.
    public func amount(forExponent exponent: Int16) -> Decimal {
        decimal(fromPlanks: planks(ofExponent: exponent))
    }
}
