import Foundation
import PolkadotUI

final class MessageShortTimestampFormatter: TimestampFormatting {
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

    func string(for date: Date, now _: Date = Date()) -> String {
        timeFormatter.string(from: date)
    }
}
