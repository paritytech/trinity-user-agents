import Foundation
public import UIKit_iOS
import UIKit

public extension RoundedButton {
    struct Style {
        public let background: RoundedView.Style
        public let title: TitleStyle

        public init(background: RoundedView.Style, title: TitleStyle) {
            self.background = background
            self.title = title
        }
    }

    struct TitleStyle {
        public let normalColor: UIColor
        public let highlightedColor: UIColor
        public let font: UIFont

        public init(normalColor: UIColor, highlightedColor: UIColor, font: UIFont) {
            self.normalColor = normalColor
            self.highlightedColor = highlightedColor
            self.font = font
        }
    }

    func apply(style: Style) {
        roundedBackgroundView?.apply(style: style.background)
        imageWithTitleView?.titleFont = style.title.font
        imageWithTitleView?.titleColor = style.title.normalColor
        imageWithTitleView?.highlightedTitleColor = style.title.highlightedColor
    }
}
