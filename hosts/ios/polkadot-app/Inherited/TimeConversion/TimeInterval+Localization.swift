import Foundation

extension TimeInterval {
    func localizedDaysHoursOrFallbackMinutes(
        for locale: Locale = .current,
        preposition: String? = nil,
        separator: String = " ",
        shortcutHandler: PredefinedTimeShortcutProtocol? = nil,
        roundsDown: Bool = true
    ) -> String {
        let (days, hours) = getDaysAndHours(roundingDown: roundsDown)

        guard days > 0 || hours > 0 else {
            return localizedDaysHoursMinutes(
                for: locale,
                preposition: preposition ?? "",
                separator: separator,
                atLeastMinutesToShow: 1
            )
        }

        return localizedCommon(
            for: locale,
            preposition: preposition,
            separator: separator,
            shortcutHandler: shortcutHandler,
            roundsDown: roundsDown,
            daysFormat: { String(localized: .Time.commonDays(value: $0)) },
            hoursFormat: { String(localized: .Time.commonHours(value: $0)) }
        )
    }

    func localizedDaysHours(
        for locale: Locale = .current,
        preposition: String? = nil,
        separator: String = " ",
        shortcutHandler: PredefinedTimeShortcutProtocol? = nil,
        roundsDown: Bool = true
    ) -> String {
        localizedCommon(
            for: locale,
            preposition: preposition,
            separator: separator,
            shortcutHandler: shortcutHandler,
            roundsDown: roundsDown,
            daysFormat: { String(localized: .Time.commonDays(value: $0)) },
            hoursFormat: { String(localized: .Time.commonHours(value: $0)) }
        )
    }

    func localizedDaysHoursMinutes(
        for _: Locale = .current,
        preposition: String = "",
        separator: String = " ",
        atLeastMinutesToShow: Int? = nil
    ) -> String {
        let days = daysFromSeconds
        let hours = (self - TimeInterval(days).secondsFromDays).hoursFromSeconds
        let minutes = (
            self - TimeInterval(days).secondsFromDays -
                TimeInterval(hours).secondsFromHours
        ).minutesFromSeconds

        var components: [String] = []

        if days > 0 {
            let daysString = String(localized: .Time.commonDays(value: days))

            components.append(daysString)
        }

        if hours > 0 {
            let hoursString = String(localized: .Time.commonHours(value: hours))

            components.append(hoursString)
        }

        if minutes > 0, components.count < 2 {
            let minutesString = String(localized: .Time.commonMinutes(value: minutes))

            components.append(minutesString)
        }

        if components.isEmpty, let minutes = atLeastMinutesToShow {
            let minutesString = String(localized: .Time.commonMinutes(value: minutes))

            components.append(minutesString)
        }

        let timeString = components.joined(separator: separator)

        if !preposition.isEmpty {
            return preposition + " " + timeString
        } else {
            return timeString
        }
    }

    func localizedDaysHoursIncludingZero(for locale: Locale) -> String {
        let days = daysFromSeconds
        let hours = (self - TimeInterval(days).secondsFromDays).hoursFromSeconds

        guard days > 0 || hours > 0 else {
            return String(localized: .Time.commonDays(value: 0))
        }

        return localizedDaysHours(for: locale)
    }

    func localizedFractionDays(for locale: Locale, shouldAnnotate: Bool) -> String {
        let days = fractionDaysFromSeconds
        let formatter = NumberFormatter.decimalFormatter(precision: 1, rounding: .down)
        formatter.locale = locale
        let optDaysString = formatter.stringFromDecimal(days)

        guard shouldAnnotate else {
            return optDaysString ?? ""
        }
        guard let daysString = optDaysString else {
            return ""
        }
        return String(localized: .Time.commonDaysFraction(value: daysString))
    }

    func localizedDaysHoursOrTime(for locale: Locale) -> String? {
        let days = daysFromSeconds

        if days > 0 {
            return localizedDaysHours(for: locale)
        } else {
            let formatter = DateComponentsFormatter.fullTime
            return formatter.value(for: locale).string(from: self)
        }
    }

    // swiftlint:disable:next function_parameter_count
    private func localizedCommon(
        for locale: Locale,
        preposition: String?,
        separator: String,
        shortcutHandler: PredefinedTimeShortcutProtocol?,
        roundsDown: Bool,
        daysFormat: (Int) -> String,
        hoursFormat: (Int) -> String
    ) -> String {
        if
            let shortcut = shortcutHandler?.getShortcut(
                for: self,
                roundsDown: roundsDown,
                locale: locale
            ) {
            return shortcut
        }

        let (days, hours) = getDaysAndHours(roundingDown: roundsDown)

        var components: [String] = []
        if days > 0 {
            let daysString = daysFormat(days)
            components.append(daysString)
        }

        if hours > 0 {
            let hoursString = hoursFormat(hours)
            components.append(hoursString)
        }

        let timeString = components.joined(separator: separator)
        guard let preposition, !preposition.isEmpty else {
            return timeString
        }
        return preposition + " " + timeString
    }

    func localizedStakingDaysHours(
        for locale: Locale = .current,
        preposition: String? = nil,
        separator: String = " ",
        shortcutHandler: PredefinedTimeShortcutProtocol? = nil,
        roundsDown: Bool = true
    ) -> String {
        let (days, hours) = getDaysAndHours(roundingDown: roundsDown)

        guard days > 0 || hours > 0 else {
            return localizedDaysHoursMinutes(
                for: locale,
                preposition: preposition ?? "",
                separator: separator,
                atLeastMinutesToShow: 1
            )
        }

        return localizedCommon(
            for: locale,
            preposition: preposition,
            separator: separator,
            shortcutHandler: shortcutHandler,
            roundsDown: roundsDown,
            daysFormat: { String(localized: .Time.commonDays(value: $0)) },
            hoursFormat: { String(localized: .Time.commonHours(value: $0)) }
        )
    }
}

extension UInt {
    func localizedDaysPeriod(for _: Locale) -> String {
        guard self == 1 else {
            return String(localized: .Time.commonEveryDays(value: Int(bitPattern: self)))
        }
        return String(localized: .Time.commonEveryday)
    }
}
