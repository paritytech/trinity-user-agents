import Foundation
import UIKit_iOS
import UIKitExt

enum BottomNotificationFactory {
    static var defaultConfiguration: ModalSheetPresentationConfiguration {
        let appearanceAnimator = BlockViewAnimator(
            duration: 0.15,
            delay: 0.0,
            options: [.curveEaseOut]
        )
        let dismissalAnimator = BlockViewAnimator(
            duration: 0.15,
            delay: 0.0,
            options: [.curveLinear]
        )

        let configuration = ModalSheetPresentationConfiguration(
            contentAppearanceAnimator: appearanceAnimator,
            contentDissmisalAnimator: dismissalAnimator,
            style: .init(sizing: .manual, backdropColor: .clear),
            extendUnderSafeArea: false,
            dismissFinishSpeedFactor: 0.6,
            dismissCancelSpeedFactor: 0.6
        )

        return configuration
    }

    static func createMessageNotification(for title: String) -> ControllerBackedProtocol? {
        let factory = ModalSheetPresentationFactory(configuration: Self.defaultConfiguration)

        let controller = BottomNotificationViewController(notificationTitle: title)
        controller.modalTransitioningFactory = factory
        controller.modalPresentationStyle = .custom

        let estimatedHeight = BottomNotificationLayout.estimateContentHeight(for: title)
        let preferredHeight: CGFloat = max(estimatedHeight, 52)

        let totalHeight = BottomNotificationConstants.contentInsets.bottom +
            BottomNotificationConstants.contentInsets.top + preferredHeight
        controller.preferredContentSize = CGSize(width: 0, height: totalHeight)

        return controller
    }
}
