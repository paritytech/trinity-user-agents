import Foundation
import Foundation_iOS

extension DateComponentsFormatter {
    static var fullTime: LocalizableResource<DateComponentsFormatter> {
        LocalizableResource { locale in
            var calendar = Calendar.current
            calendar.locale = locale
            let dateFormatter = DateComponentsFormatter()
            dateFormatter.allowedUnits = [.hour, .minute, .second]
            dateFormatter.unitsStyle = .positional
            dateFormatter.zeroFormattingBehavior = .pad
            dateFormatter.calendar = calendar

            return dateFormatter
        }
    }

    static var minuteSeconds: LocalizableResource<DateComponentsFormatter> {
        LocalizableResource { locale in
            var calendar = Calendar.current
            calendar.locale = locale
            let dateFormatter = DateComponentsFormatter()
            dateFormatter.allowedUnits = [.minute, .second]
            dateFormatter.unitsStyle = .positional
            dateFormatter.zeroFormattingBehavior = .pad
            dateFormatter.calendar = calendar

            return dateFormatter
        }
    }
}
