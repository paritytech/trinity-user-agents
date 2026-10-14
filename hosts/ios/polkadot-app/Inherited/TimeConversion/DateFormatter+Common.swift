import Foundation
import Foundation_iOS

extension DateFormatter {
    static func localizableFormatterFromTemplate(_ template: String) -> LocalizableResource<DateFormatter> {
        LocalizableResource { locale in
            let format = DateFormatter.dateFormat(fromTemplate: template, options: 0, locale: locale)
            let dateFormatter = DateFormatter()
            dateFormatter.dateFormat = format
            dateFormatter.locale = locale
            return dateFormatter
        }
    }

    static var shortMonthDay: LocalizableResource<DateFormatter> {
        localizableFormatterFromTemplate("MMM dd")
    }

    static var minutesSeconds: LocalizableResource<DateFormatter> {
        localizableFormatterFromTemplate("mm:SS")
    }

    static var fullMonth: LocalizableResource<DateFormatter> {
        localizableFormatterFromTemplate("MMMM")
    }

    static var dayShortMonthYear: LocalizableResource<DateFormatter> {
        localizableFormatterFromTemplate("dd MMM YYYY")
    }

    static var fullMonthDay: LocalizableResource<DateFormatter> {
        localizableFormatterFromTemplate("MMMM d")
    }
}
