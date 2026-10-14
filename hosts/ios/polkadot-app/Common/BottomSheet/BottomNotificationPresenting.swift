import UIKit
import UIKitExt

protocol BottomNotificationPresenting {
    func presentSuccessNotification(
        _ title: String,
        from view: ControllerBackedProtocol?,
        completion closure: (() -> Void)?
    )
}

extension BottomNotificationPresenting {
    @MainActor
    func presentSuccessNotification(_ title: String, from view: ControllerBackedProtocol?) {
        presentSuccessNotification(title, from: view, completion: nil)
    }

    @MainActor
    func presentSuccessNotification(
        _ title: String,
        from view: ControllerBackedProtocol?,
        completion closure: (() -> Void)?
    ) {
        presentSuccessNotification(
            title,
            from: view?.controller,
            completion: closure
        )
    }

    @MainActor
    func presentSuccessNotification(
        _ title: String,
        from presenter: UIViewController?,
        completion closure: (() -> Void)?
    ) {
        guard let notificationView = BottomNotificationFactory.createMessageNotification(for: title) else {
            return
        }

        presenter?.present(notificationView.controller, animated: true, completion: closure)
    }
}
