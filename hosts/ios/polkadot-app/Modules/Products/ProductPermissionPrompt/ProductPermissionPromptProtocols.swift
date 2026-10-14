import UIKit
import UIKit_iOS
import PolkadotUI

class ProductPromptStyler: MessageSheetStyling {
    var controlFactory: MessageSheetControlFactoryProtocol {
        ProductPromptControlFactory()
    }

    func applyStyle(to view: MessageSheetStyleAcceptable) {
        view.backgroundView.applyBackgroundStyle(
            .bgSurfaceContainer,
            cornerRadius: BottomSheetStyleConstants.cornerRadius
        )
        view.backgroundInsets = BottomSheetStyleConstants.backgroundInsets
        view.contentInsets = UIEdgeInsets(top: 24, left: 16, bottom: 16, right: 16)

        view.titleLabel.textColor = .fgPrimary
        view.titleLabel.font = .semibold24
        view.titleLabel.textAlignment = .center
        view.titleLabel.numberOfLines = 3
        view.titleLabel.adjustsFontSizeToFitWidth = true
        view.titleLabel.minimumScaleFactor = 0.6

        view.detailsLabel.textColor = .fgSecondary
        view.detailsLabel.font = .regular16
        view.detailsLabel.textAlignment = .center

        view.afterGraphicsSpacing = 16
        view.afterTitleSpacing = 12
        view.afterDetailsSpacing = 32
        view.buttonsAxis = .vertical
        view.buttonsOrder = .mainSecondary
        view.buttonsSpacing = DSButtonStyle.Size.mediumIncreased.verticalPadding
        view.actionHeight = DSButtonStyle.Size.mediumIncreased.height
    }
}

private final class ProductPromptControlFactory: MessageSheetControlFactoryProtocol {
    func createMain() -> MessageSheetControl {
        DSButtonView("", style: .primary, size: .mediumIncreased, expands: true)
    }

    func createSecondary() -> MessageSheetControl {
        DSButtonView("", style: .secondary, size: .mediumIncreased, expands: true)
    }

    func createTertiary() -> MessageSheetControl {
        DSButtonView("", style: .tertiary, size: .mediumIncreased, expands: true)
    }
}
