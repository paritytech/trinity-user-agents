import Foundation
import PolkadotUI

final class MessageTimestampFormatter: TimestampFormatting {
    private let locale: Locale
    private let calendar: Calendar

    private lazy var timeFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = locale
        formatter.calendar = calendar
        formatter.dateFormat = "H:mm" // 1:33 (no leading zero, 24h format)
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

        // < 1 minute: "Now"
        if elapsed < .secondsInMinute {
            return .init(localized: .chatTimestampNow)
        }

        // < 1 hour: "5m"
        if elapsed < .secondsInHour {
            let minutes = Int32(elapsed / .secondsInMinute)
            return .init(localized: .chatTimestampMinutes(minutes))
        }

        // >= 1 hour: "1:33" (24h format, no leading zero)
        return timeFormatter.string(from: date)
    }
}

// MARK: - Preview

#if DEBUG
    import SwiftUI

    private struct MessageTimestampTestCase: Identifiable {
        let id = UUID()
        let label: String
        let date: Date
        let expected: String
    }

    private struct MessageTimestampFormatterPreview: View {
        let formatter = MessageTimestampFormatter()
        let now = Date()

        var testCases: [MessageTimestampTestCase] {
            let calendar = Calendar.current
            let hour1 = calendar.date(bySettingHour: 1, minute: 33, second: 0, of: now)!
            let hour9 = calendar.date(bySettingHour: 9, minute: 5, second: 0, of: now)!
            let hour13 = calendar.date(bySettingHour: 13, minute: 45, second: 0, of: now)!
            let hour23 = calendar.date(bySettingHour: 23, minute: 59, second: 0, of: now)!

            return [
                MessageTimestampTestCase(
                    label: "Future (clock skew)",
                    date: now.addingTimeInterval(60),
                    expected: "Now"
                ),
                MessageTimestampTestCase(
                    label: "Just now (0 sec)",
                    date: now,
                    expected: "Now"
                ),
                MessageTimestampTestCase(
                    label: "30 seconds ago",
                    date: now.addingTimeInterval(-30),
                    expected: "Now"
                ),
                MessageTimestampTestCase(
                    label: "1 minute ago",
                    date: now.addingTimeInterval(-60),
                    expected: "1m"
                ),
                MessageTimestampTestCase(
                    label: "5 minutes ago",
                    date: now.addingTimeInterval(-5 * 60),
                    expected: "5m"
                ),
                MessageTimestampTestCase(
                    label: "59 minutes ago",
                    date: now.addingTimeInterval(-59 * 60),
                    expected: "59m"
                ),
                MessageTimestampTestCase(
                    label: "1:33 (no leading zero)",
                    date: hour1,
                    expected: "1:33"
                ),
                MessageTimestampTestCase(
                    label: "9:05 (no leading zero hour)",
                    date: hour9,
                    expected: "9:05"
                ),
                MessageTimestampTestCase(
                    label: "13:45 (afternoon)",
                    date: hour13,
                    expected: "13:45"
                ),
                MessageTimestampTestCase(
                    label: "23:59 (late night)",
                    date: hour23,
                    expected: "23:59"
                )
            ]
        }

        var body: some View {
            List {
                Section("Message Timestamp Test Cases") {
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

    #Preview("Message Timestamp Formats") {
        MessageTimestampFormatterPreview()
    }
#endif
