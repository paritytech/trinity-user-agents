import Foundation
import UIKit_iOS
import UIKit
import PolkadotUI
import DesignSystem

final class DeviceStatusSheetStyler: MessageSheetStyling {
    var controlFactory: MessageSheetControlFactoryProtocol {
        DeviceStatusControlFactory()
    }

    func applyStyle(to view: MessageSheetStyleAcceptable) {
        view.backgroundView.applyBackgroundStyle(
            .bgSurfaceContainer,
            cornerRadius: BottomSheetStyleConstants.cornerRadius
        )

        view.backgroundInsets = PageSheetStyleConstants.backgroundInsets
        view.contentInsets = UIEdgeInsets(top: 32, left: 16, bottom: 16, right: 16)
        view.titleLabel.typography = .headlineSmall
        view.titleLabel.textColor = .fgPrimary
        view.titleLabel.textAlignment = .left

        view.detailsLabel.typography = .paragraphLarge
        view.detailsLabel.textColor = .fgTertiary
        view.detailsLabel.textAlignment = .left

        view.afterGraphicsSpacing = 32
        view.afterTitleSpacing = 8
        view.afterDetailsSpacing = 16
        view.buttonsAxis = .vertical
        view.buttonsOrder = .mainSecondary
        view.buttonsSpacing = DSButtonStyle.Size.mediumIncreased.verticalPadding
        view.actionHeight = DSButtonStyle.Size.mediumIncreased.height
    }
}

final class DeviceStatusControlFactory: MessageSheetControlFactoryProtocol {
    func createMain() -> MessageSheetControl {
        DSButtonView("", style: .primary, size: .mediumIncreased, expands: true)
    }

    func createSecondary() -> MessageSheetControl {
        DSButtonView("", style: .tertiary, size: .mediumIncreased, expands: true)
    }
}

final class SwitchConfirmationSheetStyler: MessageSheetStyling {
    var controlFactory: MessageSheetControlFactoryProtocol {
        SwitchConfirmationControlFactory()
    }

    func applyStyle(to view: MessageSheetStyleAcceptable) {
        view.backgroundView.applyBackgroundStyle(
            .bgSurfaceContainer,
            cornerRadius: BottomSheetStyleConstants.cornerRadius
        )

        view.backgroundInsets = PageSheetStyleConstants.backgroundInsets
        view.contentInsets = UIEdgeInsets(top: 24, left: 16, bottom: 16, right: 16)
        view.titleLabel.typography = .headlineSmall
        view.titleLabel.textColor = .fgPrimary
        view.titleLabel.textAlignment = .center

        view.detailsLabel.typography = .paragraphLarge
        view.detailsLabel.textColor = .fgTertiary
        view.detailsLabel.textAlignment = .center

        view.afterTitleSpacing = 12
        view.afterDetailsSpacing = 34
        view.buttonsAxis = .horizontal
        view.buttonsOrder = .secondaryMain
        view.buttonsSpacing = DSButtonStyle.Size.large.horizontalPadding
        view.actionHeight = DSButtonStyle.Size.large.height
    }
}

final class SwitchConfirmationControlFactory: MessageSheetControlFactoryProtocol {
    func createMain() -> MessageSheetControl {
        let button = RoundedButton()
        button.applyDestructiveStyle()
        button.imageWithTitleView?.titleFont = UIFont.titleMedium
        return button
    }

    func createSecondary() -> MessageSheetControl {
        let button = RoundedButton()
        button.applyTitleTertiaryStyle()
        button.imageWithTitleView?.titleFont = UIFont.titleMedium
        return button
    }
}
