@testable import polkadot_app
import Foundation
import Testing

final class ChatDateSeparatorFormatterTests {
    private var formatter: ChatDateSeparatorFormatter!
    private let now = Date()

    init() {
        formatter = ChatDateSeparatorFormatter()
    }

    @Test("Returns 'Today' for current date")
    func today() {
        let result = formatter.string(for: now, now: now)
        #expect(result == "Today")
    }

    @Test("Returns 'Yesterday' for previous calendar day")
    func yesterday() {
        let calendar = Calendar.current
        let yesterday = calendar.date(byAdding: .day, value: -1, to: calendar.startOfDay(for: now))!
        let result = formatter.string(for: yesterday, now: now)
        #expect(result == "Yesterday", "Expected 'Yesterday', got: \(result)")
    }

    @Test("Returns date format without year for 2 days ago")
    func twoDaysAgo() {
        let calendar = Calendar.current
        let date = calendar.date(byAdding: .day, value: -2, to: now)!
        let result = formatter.string(for: date, now: now)

        #expect(result.contains(","), "Expected comma separator, got: \(result)")
        #expect(!result.contains("202"), "Should not contain year, got: \(result)")
    }

    @Test("Returns date format without year for 1 week ago")
    func oneWeekAgo() {
        let calendar = Calendar.current
        let date = calendar.date(byAdding: .day, value: -7, to: now)!
        let result = formatter.string(for: date, now: now)

        #expect(!result.contains("202"), "Should not contain year, got: \(result)")
    }

    @Test("Returns date format without year for date in same year")
    func sameYearDate() {
        let calendar = Calendar.current
        let currentYear = calendar.component(.year, from: now)
        let date = calendar.date(from: DateComponents(
            year: currentYear,
            month: 1,
            day: 5
        ))!

        if !calendar.isDateInToday(date), !calendar.isDateInYesterday(date) {
            let result = formatter.string(for: date, now: now)
            #expect(!result.contains("\(currentYear)"), "Should not contain year for same year date, got: \(result)")
        }
    }

    @Test("Returns date format with year for last year")
    func lastYear() {
        let calendar = Calendar.current
        let date = calendar.date(byAdding: .year, value: -1, to: now)!
        let result = formatter.string(for: date, now: now)

        let yearPattern = #/\d{4}/#
        #expect(result.contains(yearPattern), "Expected year in format, got: \(result)")
    }

    @Test("Returns date format with year for 2 years ago")
    func twoYearsAgo() {
        let calendar = Calendar.current
        let date = calendar.date(byAdding: .year, value: -2, to: now)!
        let result = formatter.string(for: date, now: now)

        let yearPattern = #/\d{4}/#
        #expect(result.contains(yearPattern), "Expected year in format, got: \(result)")
    }

    @Test("Different year format contains day of week, day, month, and year")
    func differentYearFullFormat() {
        let calendar = Calendar.current
        let date = calendar.date(byAdding: .year, value: -1, to: now)!
        let result = formatter.string(for: date, now: now)

        #expect(result.contains(","), "Expected comma separator, got: \(result)")

        let yearPattern = #/\d{4}/#
        #expect(result.contains(yearPattern), "Expected year, got: \(result)")

        let dayPattern = #/\d{1,2}/#
        #expect(result.contains(dayPattern), "Expected day number, got: \(result)")
    }

    @Test("Same year vs different year produces different formats")
    func sameVsDifferentYear() {
        let calendar = Calendar.current
        let currentYear = calendar.component(.year, from: now)

        let sameYearDate = calendar.date(from: DateComponents(
            year: currentYear,
            month: 1,
            day: 5
        ))!

        let differentYearDate = calendar.date(byAdding: .year, value: -1, to: sameYearDate)!

        let sameYearResult = formatter.string(for: sameYearDate, now: now)
        let differentYearResult = formatter.string(for: differentYearDate, now: now)

        #expect(
            differentYearResult.count > sameYearResult.count,
            "Different year format should be longer. Same year: \(sameYearResult), Different year: \(differentYearResult)"
        )
    }
}
