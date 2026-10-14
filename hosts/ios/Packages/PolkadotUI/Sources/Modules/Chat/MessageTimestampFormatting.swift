import Foundation

public protocol TimestampFormatting {
    func string(for date: Date, now: Date) -> String
}

public extension TimestampFormatting {
    func string(for date: Date) -> String {
        string(for: date, now: .now)
    }
}

#if DEBUG
    final class TimestampFormatter: TimestampFormatting {
        func string(for date: Date, now _: Date) -> String {
            date.description
        }
    }
#endif
