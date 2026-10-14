import UIKit
import PolkadotUI

extension NavigationBarStyle {
    static var shadowStyle: NavigationBarStyle {
        let attributes = styleAttributes()

        return NavigationBarStyle(
            backgroundColor: .bgSurfaceMain,
            shadow: nil,
            shadowColor: .strokePrimary,
            tintColor: .fgPrimary,
            backImage: Self.backIndicatorImage,
            backgroundEffect: nil,
            titleAttributes: attributes.title,
            largeTitleAttributes: attributes.largeTitle
        )
    }

    static var defaultStyle: NavigationBarStyle {
        let attributes = styleAttributes()

        return NavigationBarStyle(
            backgroundColor: .bgSurfaceMain,
            shadow: nil,
            shadowColor: nil,
            tintColor: .fgPrimary,
            backImage: Self.backIndicatorImage,
            backgroundEffect: nil,
            titleAttributes: attributes.title,
            largeTitleAttributes: attributes.largeTitle
        )
    }

    static var transparentStyle: NavigationBarStyle {
        let attributes = styleAttributes()

        return NavigationBarStyle(
            backgroundColor: nil,
            shadow: nil,
            shadowColor: nil,
            tintColor: .fgPrimary,
            backImage: Self.backIndicatorImage,
            backgroundEffect: nil,
            titleAttributes: attributes.title,
            largeTitleAttributes: attributes.largeTitle
        )
    }

    private static func styleAttributes() -> (
        title: [NSAttributedString.Key: Any],
        largeTitle: [NSAttributedString.Key: Any]
    ) {
        let normalFont = UIFont.titleLarge
        let largeFont = UIFont.titleLarge
        let textColor: UIColor = .fgPrimary

        let titleAttributes: [NSAttributedString.Key: Any] = [
            .font: normalFont,
            .foregroundColor: textColor
        ]

        let largeTitleAttributes: [NSAttributedString.Key: Any] = [
            .font: largeFont,
            .foregroundColor: textColor
        ]

        return (titleAttributes, largeTitleAttributes)
    }
}

extension NavigationBarStyle {
    static var backIndicatorImage: UIImage {
        if #available(iOS 26.0, *) {
            .buttonBack
        } else {
            .buttonBack.withAlignmentRectInsets(.left(-8))
        }
    }
}
