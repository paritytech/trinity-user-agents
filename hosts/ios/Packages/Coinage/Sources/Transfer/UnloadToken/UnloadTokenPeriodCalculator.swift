import Foundation

/// Computes valid unload token periods based on current time and runtime constant.
///
/// Both `current_period` and `old_period` are valid. If they differ,
/// the user has tokens in two periods to try.
enum UnloadTokenPeriodCalculator {
    /// Returns valid periods for unload token usage.
    ///
    /// - Parameters:
    ///   - currentDate: The current date.
    ///   - periodDuration: The duration of each period in seconds (runtime constant).
    /// - Returns: Array of 1-2 valid periods, with the current period last
    ///   (preferred because it has more time remaining).
    static func validPeriods(
        currentDate: Date,
        periodDuration: UInt64
    ) -> [UInt32] {
        guard periodDuration > 0 else { return [] }

        let nowSecs = UInt64(max(0, currentDate.timeIntervalSince1970))
        let currentPeriod = nowSecs / periodDuration

        let lookback = CoinageConstants.periodLookbackInterval
        let oneHourAgo = nowSecs >= lookback ? nowSecs - lookback : 0
        let oldPeriod = oneHourAgo / periodDuration

        guard oldPeriod == currentPeriod else {
            // Old period first (may still have available counters), current period second
            return [UInt32(oldPeriod), UInt32(currentPeriod)]
        }

        return [UInt32(currentPeriod)]
    }
}
