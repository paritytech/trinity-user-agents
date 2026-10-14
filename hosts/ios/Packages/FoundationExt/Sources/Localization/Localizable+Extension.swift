import Foundation
import Foundation_iOS

public extension Locale {
    var rLanguages: [String]? {
        [identifier]
    }
}

public extension Localizable {
    var selectedLocale: Locale { localizationManager?.selectedLocale ?? Locale.current }
}
