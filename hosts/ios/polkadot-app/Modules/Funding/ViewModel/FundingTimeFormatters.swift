import Foundation
import Foundation_iOS
import PolkadotUI

/// When a session ended: "Today at 12:45", "Yesterday at 12:45",
/// "Jun 12 at 16:40", and "Jun 12, 2025" in an earlier year.
final class FundingTimestampFormatter: TimestampFormatting {
    private let calendar: Calendar

    init(calendar: Calendar = .current) {
        self.calendar = calendar
    }

    func string(for date: Date, now: Date) -> String {
        let time = date.formatted(date: .omitted, time: .shortened)
        if calendar.isDate(date, inSameDayAs: now) {
            return String(localized: .Funding.dateTodayAt(time: time))
        }
        if isYesterday(date, now: now) {
            return String(localized: .Funding.dateYesterdayAt(time: time))
        }
        if calendar.isDate(date, equalTo: now, toGranularity: .year) {
            return String(localized: .Funding.dateDayAt(day: format(date, template: "MMMd"), time: time))
        }
        return format(date, template: "yMMMd")
    }
}

private extension FundingTimestampFormatter {
    func isYesterday(_ date: Date, now: Date) -> Bool {
        guard let yesterday = calendar.date(byAdding: .day, value: -1, to: now) else { return false }
        return calendar.isDate(date, inSameDayAs: yesterday)
    }

    /// `template` names the fields; the locale orders and punctuates them.
    func format(_ date: Date, template: String) -> String {
        let formatter = DateFormatter.localizableFormatterFromTemplate(template).value(for: calendar.locale ?? .current)
        formatter.calendar = calendar
        formatter.timeZone = calendar.timeZone
        return formatter.string(from: date)
    }
}

/// When the provider says the funds arrive: whole minutes under an hour,
/// whole hours under a day, else whole days, each rounded up.
struct FundingEtaFormatter: TimeFormatterProtocol {
    func string(from timeInterval: TimeInterval) throws -> String {
        let minutes = max(1, Int((timeInterval / 60).rounded(.up)))
        if minutes < 60 {
            return String(localized: .Funding.etaMinutes(count: minutes))
        }
        let hours = Int((Double(minutes) / 60).rounded(.up))
        if hours < 24 {
            return String(localized: .Funding.etaHours(count: hours))
        }
        return String(localized: .Funding.etaDays(count: Int((Double(hours) / 24).rounded(.up))))
    }
}
