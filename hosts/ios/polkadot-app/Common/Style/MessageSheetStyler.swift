import Foundation
import UIKit_iOS
import UIKit
import PolkadotUI
import DesignSystem

struct MessageSheetStyler {
    let backgroundStyle: RoundedView.Style
    let titleColor: UIColor
    let titleFont: UIFont
    let detailsColor: UIColor
    let detailsFont: UIFont
    let controlFactory: MessageSheetControlFactoryProtocol
    let backgroundInsets: UIEdgeInsets
    let contentInsets: UIEdgeInsets
    let verticalSpacing: CGFloat
    let horizontalSpacing: CGFloat
    let actionHeight: CGFloat
    let afterTitleSpacing: CGFloat?
    let afterDetailsSpacing: CGFloat?

    init(
        controlFactory: MessageSheetControlFactoryProtocol = RoundedButtonFactory(),
        backgroundStyle: RoundedView.Style = .init(
            shadow: .init(
                shadowOpacity: 0,
                shadowColor: nil,
                shadowRadius: nil,
                shadowOffset: nil
            ),
            fillColor: .bgSurfaceContainer,
            highlightedFillColor: .fgStaticWhite,
            rounding: .init(radius: 40, corners: .allCorners)
        ),
        titleColor: UIColor = .fgPrimary,
        titleFont: UIFont = UIFont.headlineSmall,
        detailsColor: UIColor = .fgTertiary,
        detailsFont: UIFont = UIFont.paragraphLarge,
        backgroundInsets: UIEdgeInsets = .init(top: 0, left: 8, bottom: 8, right: 8),
        contentInsets: UIEdgeInsets = .init(top: 32, left: 24, bottom: 24, right: 24),
        verticalSpacing: CGFloat = 24,
        horizontalSpacing: CGFloat = 8,
        actionHeight: CGFloat = DSButtonStyle.Size.mediumIncreased.height,
        afterTitleSpacing: CGFloat? = nil,
        afterDetailsSpacing: CGFloat? = nil
    ) {
        self.controlFactory = controlFactory
        self.backgroundStyle = backgroundStyle
        self.titleColor = titleColor
        self.titleFont = titleFont
        self.detailsColor = detailsColor
        self.detailsFont = detailsFont
        self.backgroundInsets = backgroundInsets
        self.contentInsets = contentInsets
        self.verticalSpacing = verticalSpacing
        self.horizontalSpacing = horizontalSpacing
        self.actionHeight = actionHeight
        self.afterTitleSpacing = afterTitleSpacing
        self.afterDetailsSpacing = afterDetailsSpacing
    }
}

extension MessageSheetStyler: MessageSheetStyling {
    func applyStyle(to view: MessageSheetStyleAcceptable) {
        view.backgroundView.apply(style: backgroundStyle)
        view.backgroundInsets = backgroundInsets
        view.contentInsets = contentInsets
        view.titleLabel.textColor = titleColor
        view.titleLabel.font = titleFont
        view.titleLabel.textAlignment = .center
        view.detailsLabel.textColor = detailsColor
        view.detailsLabel.font = detailsFont
        view.detailsLabel.textAlignment = .center
        view.actionHeight = actionHeight

        if let afterTitleSpacing {
            view.afterTitleSpacing = afterTitleSpacing
        }
        if let afterDetailsSpacing {
            view.afterDetailsSpacing = afterDetailsSpacing
        }
    }
}

extension MessageSheetStyler {
    static func balanceSync(
        controlFactory: MessageSheetControlFactoryProtocol = DSButtonFactory()
    ) -> MessageSheetStyler {
        MessageSheetStyler(
            controlFactory: controlFactory,
            backgroundStyle: .init(
                shadow: .init(shadowOpacity: 0, shadowColor: nil, shadowRadius: nil, shadowOffset: nil),
                fillColor: .bgSurfaceContainer,
                highlightedFillColor: .fgPrimary,
                rounding: .init(radius: 32, corners: .allCorners)
            ),
            titleColor: .fgPrimary,
            titleFont: .title24SemiBold(),
            detailsColor: .fgPrimary,
            detailsFont: UIFont.paragraphLarge,
            backgroundInsets: .init(top: 0, left: 8, bottom: 8, right: 8),
            contentInsets: .init(top: 24, left: 16, bottom: 16, right: 16),
            afterTitleSpacing: 12,
            afterDetailsSpacing: 34
        )
    }
}

extension MessageSheetStyler {
    final class RoundedButtonFactory {
        let mainStyle: RoundedButton.Style
        let secondaryStyle: RoundedButton.Style

        init(
            mainStyle: RoundedButton.Style = .mainDark,
            secondaryStyle: RoundedButton.Style = .mainDark
        ) {
            self.mainStyle = mainStyle
            self.secondaryStyle = secondaryStyle
        }
    }
}

extension MessageSheetStyler.RoundedButtonFactory: MessageSheetControlFactoryProtocol {
    func createMain() -> MessageSheetControl {
        let button = RoundedButton()
        button.apply(style: mainStyle)
        return button
    }

    func createSecondary() -> MessageSheetControl {
        let button = RoundedButton()
        button.apply(style: secondaryStyle)
        return button
    }
}

extension MessageSheetStyler {
    final class DSButtonFactory {}
}

extension MessageSheetStyler.DSButtonFactory: MessageSheetControlFactoryProtocol {
    func createMain() -> MessageSheetControl {
        DSButtonView("", style: .primary, size: .mediumIncreased, expands: true)
    }

    func createSecondary() -> MessageSheetControl {
        DSButtonView("", style: .secondary, size: .mediumIncreased, expands: true)
    }
}
