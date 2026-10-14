import Foundation
import BigInt

/// A helper that finds coin combinations summing to an exact target.
///
/// Because coin denominations are powers of 2, the problem reduces to matching
/// the binary representation of the target: greedily pick the largest coin that
/// fits, which always yields the minimum number of coins. Ties in coin count are
/// broken by preferring older coins (higher age values).
///
/// Complexity: O(n log n) for the initial sort, O(n) for the greedy pass.
enum SubsetSumSolver {
    /// Finds coins that sum exactly to target, or nil if no combination exists.
    ///
    /// - Parameters:
    ///   - target: The exact amount to match, in planks (BigUInt)
    ///   - coins: Available coins to select from (denominations must be powers of 2)
    ///   - breakdownContext: Denomination breakdown context to calculate right amount for every coin
    /// - Returns: The combination with fewest coins that sums to target, or nil if none exists.
    ///   For ties in coin count, prefers older coins first (higher age).
    static func findExactMatch(
        target: BigUInt,
        from coins: [Coin],
        breakdownContext: DenominationBreakdownContext
    ) -> [Coin]? {
        guard target > 0 else {
            return []
        }
        guard !coins.isEmpty else {
            return nil
        }

        // Group coins by exponent, largest first.
        // Within each group keep coins sorted by age descending (older = preferred).
        let groups: [(value: BigUInt, coins: [Coin])] = Dictionary(grouping: coins, by: \.exponent)
            .sorted { $0.key > $1.key }
            .map { exponent, group in
                let value = breakdownContext.valueInPlanks(for: exponent)
                let sorted = group.sorted {
                    switch ($0.age, $1.age) {
                    case let (l?, r?): l > r
                    case (nil, .some): false
                    case (.some, nil): true
                    case (nil, nil): false
                    }
                }
                return (value, sorted)
            }

        // Greedy pass: for each denomination (largest → smallest) take as many
        // coins as needed. Because denominations are powers of 2 this always
        // produces the minimum-coin exact match, or proves none exists.
        var remaining = target
        var result: [Coin] = []

        for group in groups {
            guard remaining > 0 else { break }
            guard group.value > 0, group.value <= remaining else { continue }

            let quotient = remaining / group.value
            let take = min(group.coins.count, Int(clamping: quotient))

            result.append(contentsOf: group.coins.prefix(take))
            remaining -= group.value * BigUInt(take)
        }

        return remaining == 0 ? result : nil
    }
}
