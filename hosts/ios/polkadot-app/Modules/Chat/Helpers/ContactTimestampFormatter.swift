import Foundation
import PolkadotUI

final class ContactTimestampFormatter: TimestampFormatting {
    private let locale: Locale
    private let calendar: Calendar

    private lazy var weekdayFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = locale
        formatter.calendar = calendar
        formatter.dateFormat = "EEEE"
        return formatter
    }()

    private lazy var dayMonthFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = locale
        formatter.calendar = calendar
        formatter.dateFormat = "d.MM"
        return formatter
    }()

    private lazy var dayMonthYearFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = locale
        formatter.calendar = calendar
        formatter.dateFormat = "d.MM.yyyy"
        return formatter
    }()

    init(locale: Locale = .current, calendar: Calendar = .current) {
        var calendar = calendar
        calendar.locale = locale

        self.locale = locale
        self.calendar = calendar
    }

    func string(for date: Date, now: Date = Date()) -> String {
        let elapsed = now.timeIntervalSince(date)

        // Future timestamps or < 1 minute: "Now"
        if elapsed < .secondsInMinute {
            return .init(localized: .chatTimestampNow)
        }

        // < 1 hour: "5m"
        if elapsed < .secondsInHour {
            let minutes = Int32(elapsed / .secondsInMinute)
            return .init(localized: .chatTimestampMinutes(minutes))
        }

        // >= 1 hour, < 24 hours: "11h"
        if elapsed < .secondsInDay {
            let hours = Int32(elapsed / .secondsInHour)
            return .init(localized: .chatTimestampHours(hours))
        }

        // Yesterday (calendar day comparison)
        if calendar.isDateInYesterday(date) {
            return .init(localized: .chatTimestampYesterday)
        }

        // < 7 days: Day of week
        if let days = calendar.dateComponents([.day], from: date, to: now).day,
           days < 7 {
            return weekdayFormatter.string(from: date)
        }

        // < 1 year: Date without year
        if let years = calendar.dateComponents([.year], from: date, to: now).year,
           years < 1 {
            return dayMonthFormatter.string(from: date)
        }

        // >= 1 year: Date with year
        return dayMonthYearFormatter.string(from: date)
    }
}

// MARK: - Preview

#if DEBUG
    import SwiftUI

    private struct TimestampTestCase: Identifiable {
        let id = UUID()
        let label: String
        let date: Date
        let expected: String
    }

    private struct ContactTimestampFormatterPreview: View {
        let formatter = ContactTimestampFormatter()
        let now = Date()

        var testCases: [TimestampTestCase] {
            [
                TimestampTestCase(
                    label: "Future (clock skew)",
                    date: now.addingTimeInterval(60),
                    expected: "Now"
                ),
                TimestampTestCase(
                    label: "Just now (0 sec)",
                    date: now,
                    expected: "Now"
                ),
                TimestampTestCase(
                    label: "30 seconds ago",
                    date: now.addingTimeInterval(-30),
                    expected: "Now"
                ),
                TimestampTestCase(
                    label: "1 minute ago",
                    date: now.addingTimeInterval(-60),
                    expected: "1m"
                ),
                TimestampTestCase(
                    label: "5 minutes ago",
                    date: now.addingTimeInterval(-5 * 60),
                    expected: "5m"
                ),
                TimestampTestCase(
                    label: "59 minutes ago",
                    date: now.addingTimeInterval(-59 * 60),
                    expected: "59m"
                ),
                TimestampTestCase(
                    label: "1 hour ago",
                    date: now.addingTimeInterval(-60 * 60),
                    expected: "1h"
                ),
                TimestampTestCase(
                    label: "11 hours ago",
                    date: now.addingTimeInterval(-11 * 60 * 60),
                    expected: "11h"
                ),
                TimestampTestCase(
                    label: "23 hours ago",
                    date: now.addingTimeInterval(-23 * 60 * 60),
                    expected: "23h"
                ),
                TimestampTestCase(
                    label: "Yesterday (calendar)",
                    date: Calendar.current.date(byAdding: .day, value: -1, to: now)!,
                    expected: "Yesterday"
                ),
                TimestampTestCase(
                    label: "2 days ago",
                    date: now.addingTimeInterval(-2 * 24 * 60 * 60),
                    expected: "Day name"
                ),
                TimestampTestCase(
                    label: "5 days ago",
                    date: now.addingTimeInterval(-5 * 24 * 60 * 60),
                    expected: "Day name"
                ),
                TimestampTestCase(
                    label: "10 days ago",
                    date: now.addingTimeInterval(-10 * 24 * 60 * 60),
                    expected: "d.MM"
                ),
                TimestampTestCase(
                    label: "6 months ago",
                    date: now.addingTimeInterval(-180 * 24 * 60 * 60),
                    expected: "d.MM"
                ),
                TimestampTestCase(
                    label: "1 year ago",
                    date: now.addingTimeInterval(-365 * 24 * 60 * 60),
                    expected: "d.MM.yyyy"
                ),
                TimestampTestCase(
                    label: "2 years ago",
                    date: now.addingTimeInterval(-730 * 24 * 60 * 60),
                    expected: "d.MM.yyyy"
                )
            ]
        }

        var body: some View {
            List {
                Section("Timestamp Format Test Cases") {
                    ForEach(testCases) { testCase in
                        HStack {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(verbatim: testCase.label)
                                    .font(.subheadline)
                                Text(verbatim: "Expected: \(testCase.expected)")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }

                            Spacer()

                            Text(formatter.string(for: testCase.date, now: now))
                                .font(.body.bold())
                                .foregroundStyle(.blue)
                        }
                        .padding(.vertical, 4)
                    }
                }
            }
        }
    }

    #Preview("Timestamp Formats") {
        ContactTimestampFormatterPreview()
    }
#endif
