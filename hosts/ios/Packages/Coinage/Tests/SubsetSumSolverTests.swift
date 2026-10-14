import Testing
import Foundation
import BigInt
@testable import Coinage

@Suite("SubsetSumSolver Tests")
struct SubsetSumSolverTests {
    // Context: unit = 10^16, precision = 18 → base unit is 0.01
    // valueInPlanks(e) = 10^16 * 2^e
    // e=0→10^16, e=1→2*10^16, e=2→4*10^16, e=3→8*10^16, ...
    private let testContext = DenominationBreakdownContext(
        unit: BigUInt(10).power(16),
        precision: 18,
        maxExponent: 14,
        minExponent: 0
    )

    /// Converts a decimal DOT amount to planks using the test context precision.
    private func planks(_ decimal: Decimal) -> BigUInt {
        decimal.toSubstrateAmount(precision: testContext.precision)!
    }

    private func makeCoin(
        exponent: Int16,
        age: Int16? = nil,
        derivationIndex: CoinageKeyIndex = 0
    ) -> Coin {
        Coin(
            exponent: exponent,
            derivationIndex: derivationIndex,
            age: age,
            publicKey: Data(repeating: UInt8(truncatingIfNeeded: derivationIndex.item), count: 32)
        )
    }

    // MARK: - Edge Cases

    @Test("takes 1")
    func select1() {
        let coins = [2, 4, 5, 6, 7, 8].map { makeCoin(exponent: $0) }

        let result = SubsetSumSolver.findExactMatch(
            target: planks(1.00),
            from: coins,
            breakdownContext: testContext
        )

        #expect(result != nil)
        #expect(result?.count == 3)
    }

    @Test("Returns empty array for zero target")
    func zeroTargetReturnsEmpty() {
        let coins = [makeCoin(exponent: 3)]

        let result = SubsetSumSolver.findExactMatch(
            target: BigUInt(0),
            from: coins,
            breakdownContext: testContext
        )

        #expect(result != nil)
        #expect(result?.isEmpty == true)
    }

    @Test("Returns nil for empty coins array")
    func emptyCoinsReturnsNil() {
        let result = SubsetSumSolver.findExactMatch(
            target: planks(0.08),
            from: [],
            breakdownContext: testContext
        )

        #expect(result == nil)
    }

    // MARK: - Single Coin Match

    @Test("Finds single coin exact match")
    func singleCoinExactMatch() {
        let coins = [makeCoin(exponent: 3)] // 0.08

        let result = SubsetSumSolver.findExactMatch(
            target: planks(0.08),
            from: coins,
            breakdownContext: testContext
        )

        #expect(result?.count == 1)
        #expect(result?[0].exponent == 3)
    }

    @Test("Returns nil when single coin does not match target")
    func singleCoinNoMatch() {
        let coins = [makeCoin(exponent: 3)] // 0.08

        let result = SubsetSumSolver.findExactMatch(
            target: planks(0.16), // exponent 4 value, not available
            from: coins,
            breakdownContext: testContext
        )

        #expect(result == nil)
    }

    // MARK: - Multiple Coin Combinations

    @Test("Finds two-coin combination")
    func twoCoinCombination() {
        let coins = [
            makeCoin(exponent: 3, derivationIndex: 1), // 0.08
            makeCoin(exponent: 2, derivationIndex: 2) // 0.04
        ]

        let result = SubsetSumSolver.findExactMatch(
            target: planks(0.12), // 0.08 + 0.04
            from: coins,
            breakdownContext: testContext
        )

        #expect(result?.count == 2)
        let sum = result?.reduce(BigUInt(0)) { $0 + testContext.valueInPlanks(for: $1.exponent) }
        #expect(sum == planks(0.12))
    }

    @Test("Finds subset from larger set of coins")
    func subsetFromLargerSet() {
        let coins = [
            makeCoin(exponent: 0, derivationIndex: 1), // 0.01
            makeCoin(exponent: 1, derivationIndex: 2), // 0.02
            makeCoin(exponent: 2, derivationIndex: 3), // 0.04
            makeCoin(exponent: 3, derivationIndex: 4), // 0.08
            makeCoin(exponent: 4, derivationIndex: 5) // 0.16
        ]

        let result = SubsetSumSolver.findExactMatch(
            target: planks(0.11), // 0.08 + 0.02 + 0.01
            from: coins,
            breakdownContext: testContext
        )

        #expect(result != nil)
        let sum = result?.reduce(BigUInt(0)) { $0 + testContext.valueInPlanks(for: $1.exponent) }
        #expect(sum == planks(0.11))
    }

    @Test("Returns nil when no combination sums to target")
    func noCombinationSumsToTarget() {
        let coins = [
            makeCoin(exponent: 2, derivationIndex: 1), // 0.04
            makeCoin(exponent: 3, derivationIndex: 2) // 0.08
        ]

        // No subset of {0.04, 0.08} sums to 0.05
        let result = SubsetSumSolver.findExactMatch(
            target: planks(0.05),
            from: coins,
            breakdownContext: testContext
        )

        #expect(result == nil)
    }

    // MARK: - Fewer Coins Preference

    @Test("Prefers fewer coins when multiple solutions exist")
    func prefersFewerCoins() {
        let coins = [
            makeCoin(exponent: 2, derivationIndex: 1), // 0.04
            makeCoin(exponent: 2, derivationIndex: 2), // 0.04
            makeCoin(exponent: 3, derivationIndex: 3) // 0.08
        ]

        // 0.08 can be matched by either [0.08] or [0.04, 0.04]
        let result = SubsetSumSolver.findExactMatch(
            target: planks(0.08),
            from: coins,
            breakdownContext: testContext
        )

        #expect(result?.count == 1)
        #expect(result?[0].exponent == 3)
    }

    @Test("Prefers one coin over three coins")
    func prefersOneCoinOverThree() {
        let coins = [
            makeCoin(exponent: 0, derivationIndex: 1), // 0.01
            makeCoin(exponent: 1, derivationIndex: 2), // 0.02
            makeCoin(exponent: 0, derivationIndex: 3), // 0.01
            makeCoin(exponent: 2, derivationIndex: 4) // 0.04
        ]

        // 0.04 can be matched by [0.04] or [0.01, 0.02, 0.01]
        let result = SubsetSumSolver.findExactMatch(
            target: planks(0.04),
            from: coins,
            breakdownContext: testContext
        )

        #expect(result?.count == 1)
        #expect(result?[0].exponent == 2)
    }

    // MARK: - Age Preference (Tiebreaking)

    @Test("Prefers older coins when coin count is equal")
    func prefersOlderCoinsOnTie() {
        // Two coins with the same exponent but different ages
        let coins = [
            makeCoin(exponent: 3, age: 5, derivationIndex: 1), // 0.08, age 5 (older)
            makeCoin(exponent: 3, age: 1, derivationIndex: 2) // 0.08, age 1 (newer)
        ]

        let result = SubsetSumSolver.findExactMatch(
            target: planks(0.08),
            from: coins,
            breakdownContext: testContext
        )

        #expect(result?.count == 1)
        #expect(result?[0].age == 5) // Should prefer older coin
    }

    @Test("Prefers coins with age over coins with nil age")
    func prefersAgedOverNilAge() {
        let coins = [
            makeCoin(exponent: 3, age: nil, derivationIndex: 1), // 0.08, nil age (newest)
            makeCoin(exponent: 3, age: 2, derivationIndex: 2) // 0.08, age 2
        ]

        let result = SubsetSumSolver.findExactMatch(
            target: planks(0.08),
            from: coins,
            breakdownContext: testContext
        )

        #expect(result?.count == 1)
        #expect(result?[0].age == 2)
    }

    @Test("Prefers higher total age in multi-coin solutions")
    func prefersHigherTotalAge() {
        // Multiple ways to make 0.12; solver should pick the pair with highest total age
        let coins = [
            makeCoin(exponent: 3, age: 10, derivationIndex: 1), // 0.08, age 10
            makeCoin(exponent: 2, age: 1, derivationIndex: 2), // 0.04, age 1
            makeCoin(exponent: 3, age: 2, derivationIndex: 3), // 0.08, age 2
            makeCoin(exponent: 2, age: 8, derivationIndex: 4) // 0.04, age 8
        ]

        let result = SubsetSumSolver.findExactMatch(
            target: planks(0.12),
            from: coins,
            breakdownContext: testContext
        )

        #expect(result?.count == 2)
        // Should pick the pair with highest total age: age 10 + age 8 = 18
        let totalAge = result?.compactMap(\.age).reduce(0, +)
        #expect(totalAge == 18)
    }

    // MARK: - All Coins Selected

    @Test("Uses all coins when their sum equals target")
    func allCoinsEqualTarget() {
        let coins = [
            makeCoin(exponent: 0, derivationIndex: 1), // 0.01
            makeCoin(exponent: 1, derivationIndex: 2), // 0.02
            makeCoin(exponent: 2, derivationIndex: 3) // 0.04
        ]

        let target = planks(Decimal(string: "0.07")!) // 0.01 + 0.02 + 0.04
        let result = SubsetSumSolver.findExactMatch(
            target: target,
            from: coins,
            breakdownContext: testContext
        )

        #expect(result?.count == 3)
        let sum = result?.reduce(BigUInt(0)) { $0 + testContext.valueInPlanks(for: $1.exponent) }
        #expect(sum == target)
    }

    // MARK: - Fractional Values

    @Test("Handles fractional denomination values")
    func fractionalDenominationValues() {
        let coins = [
            makeCoin(exponent: 7, derivationIndex: 1), // 1.28
            makeCoin(exponent: 6, derivationIndex: 2) // 0.64
        ]

        let result = SubsetSumSolver.findExactMatch(
            target: planks(1.92), // 1.28 + 0.64
            from: coins,
            breakdownContext: testContext
        )

        #expect(result?.count == 2)
        let sum = result?.reduce(BigUInt(0)) { $0 + testContext.valueInPlanks(for: $1.exponent) }
        #expect(sum == planks(1.92))
    }

    // MARK: - Duplicate Exponents

    @Test("Handles multiple coins with same exponent")
    func duplicateExponents() {
        let coins = [
            makeCoin(exponent: 2, derivationIndex: 1), // 0.04
            makeCoin(exponent: 2, derivationIndex: 2), // 0.04
            makeCoin(exponent: 2, derivationIndex: 3) // 0.04
        ]

        // 0.08 = 0.04 + 0.04 (needs exactly 2 of the 3 coins)
        let result = SubsetSumSolver.findExactMatch(
            target: planks(0.08),
            from: coins,
            breakdownContext: testContext
        )

        #expect(result?.count == 2)
        let sum = result?.reduce(BigUInt(0)) { $0 + testContext.valueInPlanks(for: $1.exponent) }
        #expect(sum == planks(0.08))
    }

    @Test("Finds exact match from 50 mixed-value coins with target 10")
    func fiftyMixedCoinsTargetTen() async throws {
        // Build 50 coins by cycling through every exponent in the context range.
        // With minExponent=0 and maxExponent=14 that is 15 distinct denominations
        // (0.01, 0.02, 0.04 … 163.84), each appearing 3–4 times in the set.
        let exponents = Array(testContext.minExponent ... testContext.maxExponent)
        var coins: [Coin] = []
        for i in 0 ..< 50 {
            let exponent = exponents[i % exponents.count]
            coins.append(makeCoin(exponent: exponent, derivationIndex: CoinageKeyIndex.harness(UInt64(i + 1))))
        }

        struct TimedOut: Error {}
        let context = testContext
        let targetPlanks = Decimal(10).toSubstrateAmount(precision: context.precision)!

        let result: [Coin]?
        do {
            result = try await withThrowingTaskGroup(of: [Coin]?.self) { group in
                group.addTask {
                    SubsetSumSolver.findExactMatch(target: targetPlanks, from: coins, breakdownContext: context)
                }
                group.addTask {
                    try await Task.sleep(for: .seconds(2))
                    throw TimedOut()
                }
                defer { group.cancelAll() }
                return try await group.next() ?? nil
            }
        } catch is TimedOut {
            Issue.record("findExactMatch exceeded 2-second time limit")
            return
        }

        #expect(result != nil)
        let sum = result?.reduce(BigUInt(0)) { $0 + context.valueInPlanks(for: $1.exponent) }
        #expect(sum == targetPlanks)
        #expect(result?.count == 6)
    }

    @Test("Returns nil when target exceeds total of all coins")
    func targetExceedsTotalReturnsNil() {
        let coins = [
            makeCoin(exponent: 1, derivationIndex: 1), // 0.02
            makeCoin(exponent: 2, derivationIndex: 2) // 0.04
        ]

        // Total = 0.06, target = 0.10
        let result = SubsetSumSolver.findExactMatch(
            target: planks(0.10),
            from: coins,
            breakdownContext: testContext
        )

        #expect(result == nil)
    }

    // MARK: - BigUInt safety regression

    // The original Decimal implementation crashed (EXC_BREAKPOINT) when
    // remaining / group.value exceeded Int.max: Int(truncating: huge NSDecimalNumber)
    // returned INT_MIN on ARM64, and prefix(INT_MIN) trapped.
    // BigUInt division is exact and never overflows — these tests verify the behavior.

    @Test("Returns nil for huge target that no coin can match")
    func hugeTargetReturnsNil() {
        // exponent 0 → denomination = 0.01; target ≫ 1 coin
        let hugeTarget = planks(Decimal(sign: .plus, exponent: 18, significand: 1))
        let coins = [makeCoin(exponent: 0, derivationIndex: 1)]

        let result = SubsetSumSolver.findExactMatch(
            target: hugeTarget,
            from: coins,
            breakdownContext: testContext
        )

        #expect(result == nil)
    }

    @Test("Returns nil for huge target with multiple coins still insufficient")
    func hugeTargetMultipleCoinsReturnsNil() {
        let hugeTarget = planks(Decimal(sign: .plus, exponent: 18, significand: 1))
        let coins = [
            makeCoin(exponent: 0, derivationIndex: 1),
            makeCoin(exponent: 0, derivationIndex: 2),
            makeCoin(exponent: 0, derivationIndex: 3)
        ]

        let result = SubsetSumSolver.findExactMatch(
            target: hugeTarget,
            from: coins,
            breakdownContext: testContext
        )

        #expect(result == nil)
    }
}
