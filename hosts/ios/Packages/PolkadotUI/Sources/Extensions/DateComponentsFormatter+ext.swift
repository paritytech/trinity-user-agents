import Foundation

public extension DateComponentsFormatter {
    static var secondsMinutesAbbreviated: DateComponentsFormatter {
        let formatter = DateComponentsFormatter()
        formatter.allowedUnits = [.second, .minute]
        formatter.unitsStyle = .abbreviated
        return formatter
    }
}

extension DateComponentsFormatter: TimestampFormatting {
    public func string(for date: Date, now: Date) -> String {
        string(from: now, to: date) ?? ""
    }
}
