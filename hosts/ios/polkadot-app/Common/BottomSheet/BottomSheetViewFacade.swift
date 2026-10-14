import UIKit
import UIKit_iOS
import DesignSystem
import PolkadotUI

enum BottomSheetViewFacade {
    static func defaultConfiguration(autosized: Bool = false) -> ModalSheetPresentationConfiguration {
        let appearanceAnimator = BlockViewAnimator(
            duration: 0.25,
            delay: 0.0,
            options: [.curveEaseOut]
        )
        let dismissalAnimator = BlockViewAnimator(
            duration: 0.25,
            delay: 0.0,
            options: [.curveLinear]
        )

        let sizeAnimator = BlockViewAnimator(
            duration: 0.15,
            delay: 0.0,
            options: [.curveEaseOut]
        )
        let sizing = autosized ? ModalSheetPresentationStyle.Sizing.auto(maxHeight: 1) : .manual

        let configuration = ModalSheetPresentationConfiguration(
            contentAppearanceAnimator: appearanceAnimator,
            contentDissmisalAnimator: dismissalAnimator,
            contentSizeAnimator: sizeAnimator,
            style: .init(sizing: sizing, backdropColor: .bgSurfaceOverlay),
            extendUnderSafeArea: false,
            dismissFinishSpeedFactor: 0.6,
            dismissCancelSpeedFactor: 0.6
        )

        return configuration
    }

    static func setupNonNavigatingSheet(from controller: UIViewController, preferredHeight: CGFloat? = nil) {
        let factory = ModalSheetPresentationFactory(configuration: Self.defaultConfiguration(autosized: true))

        controller.modalTransitioningFactory = factory
        controller.modalPresentationStyle = .custom

        guard let preferredHeight else {
            return
        }
        let totalHeight = BottomSheetStyleConstants.backgroundInsets.bottom +
            BottomSheetStyleConstants.backgroundInsets.top + preferredHeight
        controller.preferredContentSize = CGSize(width: 0, height: totalHeight)
    }

    static func setupBottomSheet(from controller: UIViewController, preferredHeight: CGFloat? = nil) {
        let factory = AutosizedSheetPresentationFactory(configuration: Self.defaultConfiguration(autosized: true))

        controller.modalTransitioningFactory = factory
        controller.modalPresentationStyle = .custom

        if #available(iOS 26, *) {
            controller.view.cornerConfiguration = .uniformCorners(radius: .fixed(DSRadii.extraLarge))
        } else {
            controller.view.layer.masksToBounds = true
            controller.view.layer.cornerRadius = DSRadii.extraLarge
        }

        guard let preferredHeight else {
            return
        }
        let totalHeight = BottomSheetStyleConstants.backgroundInsets.bottom +
            BottomSheetStyleConstants.backgroundInsets.top + preferredHeight
        controller.preferredContentSize = CGSize(width: 0, height: totalHeight)
    }
}
