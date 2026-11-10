import Foundation
import PolkadotUI

final class CountdownDateFormatter: CountdownDateFormatting {
    private let componentUnits: Set<Calendar.Component>
    private lazy var compactMinuteSecondFormatter: DateComponentsFormatter = {
        let formatter = DateComponentsFormatter()
        formatter.allowedUnits = [.minute, .second]
        formatter.unitsStyle = .positional
        formatter.zeroFormattingBehavior = .dropLeading
        formatter.calendar = calendar
        return formatter
    }()

    init() {
        componentUnits = [.day, .hour, .minute, .second]
    }

    func formatWithSinglePart(to date: Date) -> String {
        let components = components(to: date)

        let days = components.day ?? 0
        if days > 0 { return "\(days)\(Text.day)" }

        let hours = components.hour ?? 0
        if hours > 0 { return "\(hours)\(Text.hour)" }

        let minutes = components.minute ?? 0
        if minutes > 0 { return "\(minutes)\(Text.minute)" }

        let seconds = components.second ?? 0
        if seconds > 0 { return "\(seconds)\(Text.second)" }

        return Text.defaultValue
    }

    func formatWithMultipleParts(to date: Date) -> String {
        let components = components(to: date)

        var parts = [String]()

        addPart(components.day, withSuffix: Text.day, to: &parts)
        addPart(components.hour, withSuffix: Text.hour, to: &parts)
        addPart(components.minute, withSuffix: Text.minute, to: &parts)

        let minutes = components.minute ?? 0
        let hasOnlyMinutes = parts.count == 1 && minutes > 0
        let shouldAddSeconds = parts.isEmpty || hasOnlyMinutes

        if shouldAddSeconds {
            addPart(components.second, withSuffix: Text.second, to: &parts)
        }

        if parts.isEmpty { parts.append(Text.defaultValue) }

        return parts.joined(separator: " ")
    }

    func formatCompact(to date: Date) -> String {
        let seconds = max(0, date.timeIntervalSinceNow)

        if seconds < Constants.hourInSeconds {
            return compactMinuteSecondFormatter.string(from: seconds) ?? Text.defaultValue
        }

        return formatWithMultipleParts(to: date)
    }
}

private extension CountdownDateFormatter {
    enum Constants {
        static let hourInSeconds = TimeInterval(3_600)
    }

    enum Text {
        static let day = "d"
        static let hour = "h"
        static let minute = "m"
        static let second = "s"
        static let defaultValue = "0s"
    }

    var calendar: Calendar {
        Calendar
            .localizableCurrent
            .value(for: .current)
    }

    func components(to date: Date) -> DateComponents {
        calendar.dateComponents(componentUnits, from: Date(), to: date)
    }

    func addPart(_ part: Int?, withSuffix suffix: String, to parts: inout [String]) {
        let part = part ?? 0

        if parts.isEmpty {
            if part > 0 { parts.append("\(part)\(suffix)") }
        } else {
            parts.append("\(part)\(suffix)")
        }
    }
}
