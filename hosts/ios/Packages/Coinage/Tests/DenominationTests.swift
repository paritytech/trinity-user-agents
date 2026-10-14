import Testing
import Foundation
import BigInt
@testable import Coinage

struct DenominationTests {
    // 1 cent ($0.01) at 18 decimals = 10^16 planks
    let context = DenominationBreakdownContext(
        unit: BigUInt(10).power(16),
        precision: 18,
        maxExponent: 7,
        minExponent: 0
    )

    /// `breakdown` refuses an amount it cannot reconstitute exactly rather than returning a short
    /// list. Change vouchers have to sum back to the surplus, because the call that carries no amount
    /// unloads a group's whole input value — a surplus silently dropped is paid to the destination.
    @Test("an amount the denominations cannot express exactly is reported as such")
    func inexpressibleAmountIsRejected() {
        // Smallest denomination is 2 planks, so 1 plank of the 7 cannot be expressed.
        let coarse = DenominationBreakdownContext(unit: 1, precision: 0, maxExponent: 3, minExponent: 1)

        #expect(!coarse.isExpressible(amountInPlanks: 7))
        #expect(coarse.isExpressible(amountInPlanks: 6))
    }

    @Test("zero is expressible, and every exact multiple of the smallest denomination is")
    func expressibleAmountsAreAccepted() {
        let fine = DenominationBreakdownContext(unit: 1, precision: 0, maxExponent: 3, minExponent: 0)

        #expect(fine.isExpressible(amountInPlanks: 0))
        #expect(fine.isExpressible(amountInPlanks: 7))
        #expect(fine.isExpressible(amountInPlanks: 15))
    }

    @Test("Amount property calculates the correct power of 2 for valid exponents")
    func amountProperty() {
        // unit * 2^7 = 0.01 * 128 = 1.28
        #expect(context.amount(for: Denomination(exponent: 7)) == 1.28)
        // unit * 2^4 = 0.01 * 16 = 0.16
        #expect(context.amount(for: Denomination(exponent: 4)) == 0.16)
        // unit * 2^0 = 0.01 * 1 = 0.01
        #expect(context.amount(for: Denomination(exponent: 0)) == 0.01)
    }

    @Test("Breakdown correctly decomposes a whole number into powers of 2 (Greedy)")
    func breakdownWholeNumber() throws {
        // Target: $1.50 = 150 cents
        // 150 = 128 (2^7) + 16 (2^4) + 4 (2^2) + 2 (2^1)
        let denominations = try context.breakdown(amount: 1.50)
        let exponents = denominations.map(\.exponent)

        #expect(exponents == [7, 4, 2, 1])
    }

    @Test("Breakdown handles amounts that are multiples of the base unit")
    func breakdownCents() throws {
        // 7 cents = 4 (2^2) + 2 (2^1) + 1 (2^0), given in planks: $0.07 does not survive
        // `toSubstrateAmount` at precision 18, coming back ten planks heavy, and no denomination
        // is small enough to place them.
        let sevenCents = BigUInt(10).power(16) * 7
        let denominations = try context.breakdown(amountInPlanks: sevenCents)
        let exponents = denominations.map(\.exponent)

        #expect(exponents == [2, 1, 0])
    }

    @Test("Breakdown handles amounts requiring multiple units of the maximum denomination")
    func breakdownMultipleMaxUnits() throws {
        // $2.56 = 256 cents
        // 256 = 128 (2^7) + 128 (2^7)
        let denominations = try context.breakdown(amount: 2.56)
        let exponents = denominations.map(\.exponent)

        #expect(exponents == [7, 7])
    }

    @Test("Breakdown refuses a remainder smaller than the minimum denomination ($0.01)")
    func breakdownRemainderHandling() throws {
        // $0.015 is one cent plus half a cent, and minExponent 0 makes the half unrepresentable.
        // It used to come back as [0] with the remainder dropped on the floor.
        #expect(throws: DenominationError.self) {
            try context.breakdown(amount: 0.015)
        }

        // Callers that mean to drop it say so, and get the cent.
        let rounded = context.roundedDown(amountInPlanks: BigUInt(10).power(16) * 3 / 2)
        #expect(try context.breakdown(amountInPlanks: rounded).map(\.exponent) == [0])
    }

    @Test("Breakdown refuses values below the unit, and accepts zero")
    func breakdownVerySmallAmount() throws {
        // $0.005 is smaller than the unit $0.01 (minExponent 0): nothing can carry it.
        #expect(throws: DenominationError.self) {
            try context.breakdown(amount: 0.005)
        }
        #expect(context.roundedDown(amountInPlanks: BigUInt(10).power(16) / 2) == 0)

        // Zero has nothing left over, so it breaks down to no denominations at all.
        #expect(try context.breakdown(amount: 0).isEmpty)
    }

    @Test("a ladder whose steps are exact doublings is accepted")
    func canonicalLadderIsAccepted() throws {
        // 10^16 is 2^16 * 5^16, so every step down to minExponent -16 is reached without truncating.
        try DenominationBreakdownContext(
            unit: BigUInt(10).power(16),
            precision: 18,
            maxExponent: 7,
            minExponent: -16
        ).validateLadder()

        try DenominationBreakdownContext(unit: 1, precision: 0, maxExponent: 3, minExponent: 0)
            .validateLadder()
    }

    /// The case the validation exists for. On this ladder the greedy breakdown is incomplete, so
    /// with a throwing breakdown an amount the denominations *can* carry becomes an operation that
    /// fails — which is why the configuration is refused where it is read instead.
    @Test("a ladder whose steps truncate is refused, because greedy cannot decide it")
    func truncatingLadderIsRefused() {
        // unit 5 with minExponent -1: 5 >> 1 is 2, so the steps are 5 and 2 rather than 5 and 2.5.
        let truncating = DenominationBreakdownContext(unit: 5, precision: 0, maxExponent: 0, minExponent: -1)

        #expect(throws: DenominationError.unusableLadder(.truncatingSteps(unit: 5, minExponent: -1))) {
            try truncating.validateLadder()
        }

        // 6 is 2 + 2 + 2, yet greedy takes the 5 and strands 1. Pinned so the reason the ladder is
        // refused cannot quietly stop being true.
        #expect(throws: DenominationError.inexpressible(amountInPlanks: 6, remainder: 1)) {
            try truncating.breakdown(amountInPlanks: 6)
        }
    }

    @Test("a ladder with no steps at all is refused")
    func degenerateLadderIsRefused() {
        #expect(throws: DenominationError.unusableLadder(.zeroUnit)) {
            try DenominationBreakdownContext(unit: 0, precision: 0, maxExponent: 3, minExponent: 0)
                .validateLadder()
        }

        #expect(throws: DenominationError.unusableLadder(.invertedBounds(minExponent: 3, maxExponent: 1))) {
            try DenominationBreakdownContext(unit: 1, precision: 0, maxExponent: 1, minExponent: 3)
                .validateLadder()
        }
    }

    @Test("the reported remainder is what the greedy pass could not place")
    func remainderIsReported() {
        let coarse = DenominationBreakdownContext(unit: 1, precision: 0, maxExponent: 3, minExponent: 1)

        #expect(throws: DenominationError.inexpressible(amountInPlanks: 7, remainder: 1)) {
            try coarse.breakdown(amountInPlanks: 7)
        }
        #expect(coarse.roundedDown(amountInPlanks: 7) == 6)
    }
}
