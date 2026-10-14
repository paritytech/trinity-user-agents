import Foundation

final class ChatDateSeparatorFormatter {
    private let locale: Locale
    private let calendar: Calendar

    private lazy var dateFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = locale
        formatter.calendar = calendar
        formatter.dateFormat = "EEE, d MMM"
        return formatter
    }()

    private lazy var dateWithYearFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = locale
        formatter.calendar = calendar
        formatter.dateFormat = "EEE, d MMM yyyy"
        return formatter
    }()

    init(locale: Locale = .current, calendar: Calendar = .current) {
        var calendar = calendar
        calendar.locale = locale

        self.locale = locale
        self.calendar = calendar
    }

    func string(for date: Date, now: Date = Date()) -> String {
        if calendar.isDateInToday(date) {
            return String(localized: .chatSectionToday)
        }

        if calendar.isDateInYesterday(date) {
            return String(localized: .chatSectionYesterday)
        }

        let isCurrentYear = calendar.isDate(date, equalTo: now, toGranularity: .year)

        if isCurrentYear {
            return dateFormatter.string(from: date)
        } else {
            return dateWithYearFormatter.string(from: date)
        }
    }
}

// MARK: - Preview

#if DEBUG
    import SwiftUI

    private struct DateSeparatorTestCase: Identifiable {
        let id = UUID()
        let label: String
        let date: Date
        let expected: String
    }

    private struct ChatDateSeparatorFormatterPreview: View {
        let formatter = ChatDateSeparatorFormatter()
        let now = Date()

        var testCases: [DateSeparatorTestCase] {
            let calendar = Calendar.current

            return [
                DateSeparatorTestCase(
                    label: "Today",
                    date: now,
                    expected: "Today"
                ),
                DateSeparatorTestCase(
                    label: "Yesterday",
                    date: calendar.date(byAdding: .day, value: -1, to: now)!,
                    expected: "Yesterday"
                ),
                DateSeparatorTestCase(
                    label: "2 days ago (same year)",
                    date: calendar.date(byAdding: .day, value: -2, to: now)!,
                    expected: "EEE, d MMM"
                ),
                DateSeparatorTestCase(
                    label: "1 week ago (same year)",
                    date: calendar.date(byAdding: .day, value: -7, to: now)!,
                    expected: "EEE, d MMM"
                ),
                DateSeparatorTestCase(
                    label: "Last year",
                    date: calendar.date(byAdding: .year, value: -1, to: now)!,
                    expected: "EEE, d MMM yyyy"
                ),
                DateSeparatorTestCase(
                    label: "2 years ago",
                    date: calendar.date(byAdding: .year, value: -2, to: now)!,
                    expected: "EEE, d MMM yyyy"
                )
            ]
        }

        var body: some View {
            List {
                Section("Date Separator Test Cases") {
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

    #Preview("Date Separator Formats") {
        ChatDateSeparatorFormatterPreview()
    }
#endif
