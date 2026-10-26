@testable import polkadot_app
import Foundation
import Testing

struct ContactTimestampFormatterTests {
    private let calendar: Calendar
    private let formatter: ContactTimestampFormatter
    private let now: Date

    init() throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = try #require(TimeZone(identifier: "UTC"))
        self.calendar = calendar
        formatter = ContactTimestampFormatter(locale: Locale(identifier: "en_US_POSIX"), calendar: calendar)
        now = try #require(calendar.date(from: DateComponents(year: 2_026, month: 6, day: 15, hour: 12)))
    }

    @Test("Returns 'Now' for current time")
    func nowForCurrentTime() {
        #expect(formatter.string(for: now, now: now) == "Now")
    }

    @Test("Returns 'Now' for 30 seconds ago")
    func nowFor30SecondsAgo() {
        #expect(formatter.string(for: now.addingTimeInterval(-30), now: now) == "Now")
    }

    @Test("Returns 'Now' for future timestamps (clock skew)")
    func nowForFutureTimestamp() {
        #expect(formatter.string(for: now.addingTimeInterval(60), now: now) == "Now")
    }

    @Test("Returns '1m' for exactly 1 minute ago")
    func oneMinuteAgo() {
        #expect(formatter.string(for: now.addingTimeInterval(-60), now: now) == "1m")
    }

    @Test("Returns '5m' for 5 minutes ago")
    func fiveMinutesAgo() {
        #expect(formatter.string(for: now.addingTimeInterval(-5 * 60), now: now) == "5m")
    }

    @Test("Returns '59m' for 59 minutes ago")
    func fiftyNineMinutesAgo() {
        #expect(formatter.string(for: now.addingTimeInterval(-59 * 60), now: now) == "59m")
    }

    @Test("Returns '1h' for exactly 1 hour ago")
    func oneHourAgo() {
        #expect(formatter.string(for: now.addingTimeInterval(-60 * 60), now: now) == "1h")
    }

    @Test("Returns '11h' for 11 hours ago")
    func elevenHoursAgo() {
        #expect(formatter.string(for: now.addingTimeInterval(-11 * 60 * 60), now: now) == "11h")
    }

    @Test("Returns '23h' for 23 hours ago")
    func twentyThreeHoursAgo() {
        #expect(formatter.string(for: now.addingTimeInterval(-23 * 60 * 60), now: now) == "23h")
    }

    @Test("Returns 'Yesterday' for the start of the previous day")
    func yesterday() throws {
        let date = try #require(calendar.date(byAdding: .day, value: -1, to: calendar.startOfDay(for: now)))
        #expect(formatter.string(for: date, now: now) == "Yesterday")
    }

    @Test("Returns day of week for 2 days ago")
    func twoDaysAgo() throws {
        #expect(try formatter.string(for: date(adding: .day, value: -2), now: now) == "Saturday")
    }

    @Test("Returns day of week for 5 days ago")
    func fiveDaysAgo() throws {
        #expect(try formatter.string(for: date(adding: .day, value: -5), now: now) == "Wednesday")
    }

    @Test("Returns day of week for 6 days ago")
    func sixDaysAgo() throws {
        #expect(try formatter.string(for: date(adding: .day, value: -6), now: now) == "Tuesday")
    }

    @Test("Returns d.MM format for 10 days ago")
    func tenDaysAgo() throws {
        #expect(try formatter.string(for: date(adding: .day, value: -10), now: now) == "5.06")
    }

    @Test("Returns d.MM format for 6 months ago")
    func sixMonthsAgo() throws {
        #expect(try formatter.string(for: date(adding: .month, value: -6), now: now) == "15.12")
    }

    @Test("Returns d.MM.yyyy format for 1 year ago")
    func oneYearAgo() throws {
        #expect(try formatter.string(for: date(adding: .year, value: -1), now: now) == "15.06.2025")
    }

    @Test("Returns d.MM.yyyy format for 2 years ago")
    func twoYearsAgo() throws {
        #expect(try formatter.string(for: date(adding: .year, value: -2), now: now) == "15.06.2024")
    }
}

private extension ContactTimestampFormatterTests {
    func date(adding component: Calendar.Component, value: Int) throws -> Date {
        try #require(calendar.date(byAdding: component, value: value, to: now))
    }
}
