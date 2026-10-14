@testable import polkadot_app
import Foundation
import Testing

final class ContactTimestampFormatterTests {
    private var formatter: ContactTimestampFormatter!
    private let now = Date()

    init() {
        formatter = ContactTimestampFormatter()
    }

    @Test("Returns 'Now' for current time")
    func nowForCurrentTime() {
        let result = formatter.string(for: now, now: now)
        #expect(result == "Now")
    }

    @Test("Returns 'Now' for 30 seconds ago")
    func nowFor30SecondsAgo() {
        let date = now.addingTimeInterval(-30)
        let result = formatter.string(for: date, now: now)
        #expect(result == "Now")
    }

    @Test("Returns 'Now' for future timestamps (clock skew)")
    func nowForFutureTimestamp() {
        let date = now.addingTimeInterval(60)
        let result = formatter.string(for: date, now: now)
        #expect(result == "Now")
    }

    @Test("Returns '1m' for exactly 1 minute ago")
    func oneMinuteAgo() {
        let date = now.addingTimeInterval(-60)
        let result = formatter.string(for: date, now: now)
        #expect(result == "1m")
    }

    @Test("Returns '5m' for 5 minutes ago")
    func fiveMinutesAgo() {
        let date = now.addingTimeInterval(-5 * 60)
        let result = formatter.string(for: date, now: now)
        #expect(result == "5m")
    }

    @Test("Returns '59m' for 59 minutes ago")
    func fiftyNineMinutesAgo() {
        let date = now.addingTimeInterval(-59 * 60)
        let result = formatter.string(for: date, now: now)
        #expect(result == "59m")
    }

    @Test("Returns '1h' for exactly 1 hour ago")
    func oneHourAgo() {
        let date = now.addingTimeInterval(-60 * 60)
        let result = formatter.string(for: date, now: now)
        #expect(result == "1h")
    }

    @Test("Returns '11h' for 11 hours ago")
    func elevenHoursAgo() {
        let date = now.addingTimeInterval(-11 * 60 * 60)
        let result = formatter.string(for: date, now: now)
        #expect(result == "11h")
    }

    @Test("Returns '23h' for 23 hours ago")
    func twentyThreeHoursAgo() {
        let date = now.addingTimeInterval(-23 * 60 * 60)
        let result = formatter.string(for: date, now: now)
        #expect(result == "23h")
    }

    @Test("Returns 'Yesterday' for yesterday's date")
    func yesterday() {
        let calendar = Calendar.current
        let yesterday = calendar.date(byAdding: .day, value: -1, to: calendar.startOfDay(for: now))!
        let result = formatter.string(for: yesterday, now: now)
        #expect(result == "Yesterday", "Expected 'Yesterday', got: \(result)")
    }

    @Test("Returns day of week for 2 days ago")
    func twoDaysAgo() {
        let date = now.addingTimeInterval(-2 * 24 * 60 * 60)
        let result = formatter.string(for: date, now: now)
        let weekdays = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"]
        #expect(weekdays.contains(result), "Expected day of week, got: \(result)")
    }

    @Test("Returns day of week for 5 days ago")
    func fiveDaysAgo() {
        let date = now.addingTimeInterval(-5 * 24 * 60 * 60)
        let result = formatter.string(for: date, now: now)
        let weekdays = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"]
        #expect(weekdays.contains(result), "Expected day of week, got: \(result)")
    }

    @Test("Returns day of week for 6 days ago")
    func sixDaysAgo() {
        let date = now.addingTimeInterval(-6 * 24 * 60 * 60)
        let result = formatter.string(for: date, now: now)
        let weekdays = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"]
        #expect(weekdays.contains(result), "Expected day of week, got: \(result)")
    }

    @Test("Returns d.MM format for 10 days ago")
    func tenDaysAgo() {
        let date = now.addingTimeInterval(-10 * 24 * 60 * 60)
        let result = formatter.string(for: date, now: now)
        let datePattern = #/^\d{1,2}\.\d{2}$/#
        #expect(result.contains(datePattern), "Expected d.MM format, got: \(result)")
    }

    @Test("Returns d.MM format for 6 months ago")
    func sixMonthsAgo() {
        let date = now.addingTimeInterval(-180 * 24 * 60 * 60)
        let result = formatter.string(for: date, now: now)
        #expect(result.contains("."), "Expected d.MM format, got: \(result)")
        let yearPattern = #/\.\d{4}/#
        #expect(!result.contains(yearPattern), "Should not contain year, got: \(result)")
    }

    @Test("Returns d.MM.yyyy format for 1 year ago")
    func oneYearAgo() {
        let date = now.addingTimeInterval(-365 * 24 * 60 * 60)
        let result = formatter.string(for: date, now: now)
        let yearPattern = #/\d{1,2}\.\d{2}\.\d{4}/#
        #expect(result.contains(yearPattern), "Expected d.MM.yyyy format, got: \(result)")
    }

    @Test("Returns d.MM.yyyy format for 2 years ago")
    func twoYearsAgo() {
        let date = now.addingTimeInterval(-730 * 24 * 60 * 60)
        let result = formatter.string(for: date, now: now)
        let yearPattern = #/\d{1,2}\.\d{2}\.\d{4}/#
        #expect(result.contains(yearPattern), "Expected d.MM.yyyy format, got: \(result)")
    }
}
