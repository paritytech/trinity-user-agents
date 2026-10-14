import UIKit
public import UIKit_iOS

public extension TextInputView {
    struct ClearButtonStyle {
        public let roundedButtonStyle: RoundedButton.Style
        public let icon: UIImage?
        public let contentInsets: UIEdgeInsets

        public init(
            roundedButtonStyle: RoundedButton.Style,
            icon: UIImage?,
            contentInsets: UIEdgeInsets
        ) {
            self.roundedButtonStyle = roundedButtonStyle
            self.icon = icon
            self.contentInsets = contentInsets
        }
    }

    struct Style {
        public let fieldStyle: UITextField.Style
        public let strokeOnEditing: RoundedView.Style?
        public let clearButtonStyle: ClearButtonStyle?
        public let contentInsets: UIEdgeInsets?

        public init(
            fieldStyle: UITextField.Style,
            strokeOnEditing: RoundedView.Style?,
            clearButtonStyle: ClearButtonStyle?,
            contentInsets: UIEdgeInsets?
        ) {
            self.fieldStyle = fieldStyle
            self.strokeOnEditing = strokeOnEditing
            self.clearButtonStyle = clearButtonStyle
            self.contentInsets = contentInsets
        }
    }

    func apply(style: Style) {
        textField.apply(style: style.fieldStyle)

        if let strokeOnEditing = style.strokeOnEditing {
            roundedBackgroundView?.apply(style: strokeOnEditing)
        }

        if let clearButtonStyle = style.clearButtonStyle {
            clearButton.apply(style: clearButtonStyle.roundedButtonStyle)
            clearButton.setIcon(clearButtonStyle.icon)
            clearButton.contentInsets = clearButtonStyle.contentInsets
        }

        if let contentInsets = style.contentInsets {
            self.contentInsets = contentInsets
        }
    }
}
