import Foundation
import Foundation_iOS

extension Calendar {
    static var localizableCurrent: LocalizableResource<Calendar> {
        LocalizableResource { locale in
            var calendar = Calendar.current
            calendar.locale = locale
            return calendar
        }
    }
}
