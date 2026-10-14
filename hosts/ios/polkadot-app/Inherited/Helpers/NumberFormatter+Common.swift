import Foundation
import Foundation_iOS

extension NumberFormatter {
    static var ordinal: LocalizableResource<NumberFormatter> {
        LocalizableResource { locale in
            let numberFormatter = NumberFormatter()
            numberFormatter.locale = locale
            numberFormatter.numberStyle = .ordinal
            return numberFormatter
        }
    }
}
