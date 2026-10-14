import Foundation
import SubstrateSdk
import BigInt

public struct Denomination: Equatable {
    public let exponent: Int16
}

public enum DenominationError: Error, Equatable {
    /// The amount cannot be reconstituted from denominations: `remainder` planks are left over below
    /// the smallest one.
    case inexpressible(amountInPlanks: BigUInt, remainder: BigUInt)
    /// The chain's constants do not describe a ladder the breakdown can work with. Raised where the
    /// context is loaded, so a bad configuration stops the instance rather than surfacing later as a
    /// payment that cannot be planned.
    case unusableLadder(UnusableLadder)

    public enum UnusableLadder: Equatable {
        case zeroUnit
        case invertedBounds(minExponent: Int16, maxExponent: Int16)
        /// `unit` is not divisible by `2^|minExponent|`, so ``DenominationBreakdownContext/
        /// valueInPlanks(for:)`` truncates on the way down and the steps stop being exact doublings.
        /// The greedy breakdown is only complete on a ladder that does double, so on one that does
        /// not it refuses amounts the denominations can in fact carry: unit 5 with minExponent -1
        /// gives 5 and 2, where 6 is 2 + 2 + 2 but greedy takes the 5 and strands a remainder of 1.
        case truncatingSteps(unit: BigUInt, minExponent: Int16)
    }
}

public struct DenominationBreakdownContext: Equatable {
    // Base amount which is basically unit*2^0
    // when precision is 18 and unit is 10^16 - unit value is 0.01
    let unit: BigUInt
    let precision: Int16
    let maxExponent: Int16
    let minExponent: Int16

    /// The live context is loaded from chain state (`InstanceRecord.assetUnit` plus the
    /// pallet's exponent constants). This initializer exists so callers that cannot reach
    /// the chain — SwiftUI previews, debug fixtures — can still price holdings.
    public init(unit: BigUInt, precision: Int16, maxExponent: Int16, minExponent: Int16) {
        self.unit = unit
        self.precision = precision
        self.maxExponent = maxExponent
        self.minExponent = minExponent
    }

    func breakdown(amount: Decimal) throws -> [Denomination] {
        guard let planks = amount.toSubstrateAmount(precision: precision) else {
            return []
        }
        return try breakdown(amountInPlanks: planks)
    }

    /// Converts a denomination back into its decimal currency amount.
    func amount(for denomination: Denomination) -> Decimal {
        amount(forExponent: denomination.exponent)
    }

    /// The decimal currency amount of a single holding at `exponent`.
    public func amount(forExponent exponent: Int16) -> Decimal {
        let value = valueInPlanks(for: exponent)
        return .fromSubstrateAmount(value, precision: precision) ?? 0
    }

    /// Creates a new context with updated precision from the given asset.
    /// - Parameter asset: The asset providing the new decimal precision
    /// - Returns: A new context with the asset's precision
    func withChanging(asset: AssetProtocol) -> DenominationBreakdownContext {
        DenominationBreakdownContext(
            unit: unit,
            precision: asset.decimalPrecision,
            maxExponent: maxExponent,
            minExponent: minExponent
        )
    }

    /// Whether `amount` can be reconstituted from denominations *exactly*, asked without the `do`
    /// block a caller would otherwise need just to branch.
    ///
    /// Answered by running the breakdown rather than by testing divisibility by the smallest
    /// denomination. On a ladder ``validateLadder()`` accepts the two agree, but this type's
    /// initializer is public and takes the bounds on trust, so the breakdown stays the definition
    /// for contexts that never went through the loader.
    public func isExpressible(amountInPlanks amount: BigUInt) -> Bool {
        (try? breakdown(amountInPlanks: amount)) != nil
    }

    /// The largest amount not exceeding `amount` that ``breakdown(amountInPlanks:)`` accepts.
    ///
    /// For callers that mean to drop the dust — a claim taking what it can of a remainder it will
    /// never fully cover. Pairing this with the strict breakdown keeps the rounding a decision at the
    /// call site rather than something the breakdown does silently on everyone's behalf.
    public func roundedDown(amountInPlanks amount: BigUInt) -> BigUInt {
        amount - remainderAfterBreakdown(of: amount)
    }

    /// Checks the constants describe a ladder whose every step is exactly twice the one below.
    ///
    /// ``valueInPlanks(for:)`` reaches the steps below `unit` by shifting right, which truncates.
    /// While `2^|minExponent|` divides `unit` no shift loses anything, every step is an exact
    /// doubling of the one under it, and the greedy breakdown accepts precisely the amounts the
    /// denominations can carry — greedy is a decision procedure there, not a heuristic. Let a shift
    /// truncate and that stops being true, and since ``breakdown(amountInPlanks:)`` now throws, an
    /// amount the wallet could pay becomes a payment that cannot be planned.
    ///
    /// Called once, where the constants are read from chain, so the failure names the configuration
    /// rather than arriving mid-payment.
    func validateLadder() throws {
        guard unit > 0 else {
            throw DenominationError.unusableLadder(.zeroUnit)
        }

        guard maxExponent >= minExponent else {
            throw DenominationError.unusableLadder(
                .invertedBounds(minExponent: minExponent, maxExponent: maxExponent)
            )
        }

        guard minExponent >= 0 || unit % (BigUInt(1) << Int(-minExponent)) == 0 else {
            throw DenominationError.unusableLadder(
                .truncatingSteps(unit: unit, minExponent: minExponent)
            )
        }
    }

    /// Breaks a plank amount directly into denominations, skipping the Decimal conversion.
    ///
    /// Throws rather than returning a short list. The greedy pass cannot always reach zero, and an
    /// amount that lands a few planks short is not a rounding artefact to whoever is holding the
    /// difference: the caller minting change has to sum back to the surplus exactly, and the caller
    /// minting vouchers has to deliver what it was asked for. Callers that do mean to drop the dust
    /// say so with ``roundedDown(amountInPlanks:)``.
    func breakdown(amountInPlanks amount: BigUInt) throws -> [Denomination] {
        var remaining = amount
        var results: [Denomination] = []

        for exponent in stride(from: maxExponent, through: minExponent, by: -1) {
            let value = valueInPlanks(for: exponent)
            guard value > 0 else { continue }
            while remaining >= value {
                results.append(Denomination(exponent: exponent))
                remaining -= value
            }
        }

        guard remaining == 0 else {
            throw DenominationError.inexpressible(amountInPlanks: amount, remainder: remaining)
        }

        return results
    }

    /// What the greedy pass would leave behind, without building the list.
    func remainderAfterBreakdown(of amount: BigUInt) -> BigUInt {
        var remaining = amount

        for exponent in stride(from: maxExponent, through: minExponent, by: -1) {
            let value = valueInPlanks(for: exponent)
            guard value > 0 else { continue }
            remaining %= value
            if remaining == 0 { return 0 }
        }

        return remaining
    }

    /// Converts a plank amount into its decimal currency amount using the asset precision. Public so
    /// display consumers can render ``CoinageBalance`` buckets without reaching for the precision.
    public func decimal(fromPlanks planks: Balance) -> Decimal {
        .fromSubstrateAmount(planks, precision: precision) ?? 0
    }

    /// Returns the plank value for a given denomination exponent: `unit * 2^exponent`.
    public func valueInPlanks(for exponent: Int16) -> BigUInt {
        if exponent >= 0 {
            // unit * 2^exponent
            unit << Int(exponent)
        } else {
            // unit / 2^abs(exponent)
            unit >> Int(abs(exponent))
        }
    }
}
