import UIKit
import UIKitExt

protocol BottomNotificationPresenting {
    func presentSuccessNotification(
        _ title: String,
        from view: ControllerBackedProtocol?,
        completion closure: (() -> Void)?
    )
}

@MainActor
extension BottomNotificationPresenting {
    func presentSuccessNotification(_ title: String, from view: ControllerBackedProtocol?) {
        presentSuccessNotification(title, from: view, completion: nil)
    }

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
