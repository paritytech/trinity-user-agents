import Foundation
public import UIKit_iOS
import UIKit

public extension TextWithServiceInputView {
    struct PasteButtonStyle {
        public let roundedButtonStyle: RoundedButton.Style
        public let contentInsets: UIEdgeInsets

        public init(roundedButtonStyle: RoundedButton.Style, contentInsets: UIEdgeInsets) {
            self.roundedButtonStyle = roundedButtonStyle
            self.contentInsets = contentInsets
        }
    }

    func applyPasteButton(style: PasteButtonStyle) {
        pasteButton.apply(style: style.roundedButtonStyle)
        pasteButton.contentInsets = style.contentInsets
    }
}
