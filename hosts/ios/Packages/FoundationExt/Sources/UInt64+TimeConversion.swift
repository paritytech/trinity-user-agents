import Foundation

public extension UInt64 {
    func millisecondsToSeconds() -> TimeInterval {
        let fullSeconds = self / 1_000
        let milliseconds = TimeInterval(self % 1_000) / 1_000.0

        return TimeInterval(fullSeconds) + milliseconds
    }

    func minutesToSeconds() -> TimeInterval {
        TimeInterval(self) * TimeInterval.secondsInMinute
    }

    func secondsFromDays() -> TimeInterval {
        TimeInterval(self) * TimeInterval.secondsInDay
    }
}
