import UIKit

public extension UITextField {
    struct Style {
        public let font: UIFont
        public let textColor: UIColor
        public let tintColor: UIColor?
        public let textContentType: UITextContentType?
        public let smartQuotesType: UITextSmartQuotesType
        public let smartDashesType: UITextSmartDashesType
        public let spellCheckingType: UITextSpellCheckingType

        public init(
            font: UIFont,
            textColor: UIColor,
            tintColor: UIColor? = nil,
            textContentType: UITextContentType? = nil,
            smartQuotesType: UITextSmartQuotesType = .default,
            smartDashesType: UITextSmartDashesType = .default,
            spellCheckingType: UITextSpellCheckingType = .default
        ) {
            self.font = font
            self.textColor = textColor
            self.tintColor = tintColor
            self.textContentType = textContentType
            self.smartQuotesType = smartQuotesType
            self.smartDashesType = smartDashesType
            self.spellCheckingType = spellCheckingType
        }
    }

    func apply(style: Style) {
        font = style.font
        textColor = style.textColor

        if let tintColor = style.tintColor {
            self.tintColor = tintColor
        }

        if let textContentType = style.textContentType {
            self.textContentType = textContentType
        }

        smartQuotesType = style.smartQuotesType
        smartDashesType = style.smartDashesType
        spellCheckingType = style.spellCheckingType
    }
}
