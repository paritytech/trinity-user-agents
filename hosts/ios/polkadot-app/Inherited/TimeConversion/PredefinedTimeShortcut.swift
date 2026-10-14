import Foundation

protocol PredefinedTimeShortcutProtocol {
    func getShortcut(
        for timeInterval: TimeInterval,
        roundsDown: Bool,
        locale: Locale
    ) -> String?
}

final class EverydayShortcut: PredefinedTimeShortcutProtocol {
    func getShortcut(
        for timeInterval: TimeInterval,
        roundsDown: Bool,
        locale _: Locale
    ) -> String? {
        let (days, hours) = timeInterval.getDaysAndHours(roundingDown: roundsDown)

        guard days == 1, hours == 0 else {
            return nil
        }

        return String(localized: .Time.commonEveryday)
    }
}

final class DailyShortcut: PredefinedTimeShortcutProtocol {
    func getShortcut(
        for timeInterval: TimeInterval,
        roundsDown: Bool,
        locale _: Locale
    ) -> String? {
        let (days, hours) = timeInterval.getDaysAndHours(roundingDown: roundsDown)

        guard days == 1, hours == 0 else {
            return nil
        }

        return String(localized: .Time.commonDaily)
    }
}
