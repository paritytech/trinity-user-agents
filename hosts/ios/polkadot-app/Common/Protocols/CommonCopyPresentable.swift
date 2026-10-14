import UIKit
import UIKitExt

@MainActor
protocol CommonCopyPresentable: AnyObject {
    func copyString(
        from view: ControllerBackedProtocol,
        stringToCopy: String,
        message: String
    )

    func copySensitiveString(
        from view: ControllerBackedProtocol,
        stringToCopy: String,
        message: String
    )
}

extension CommonCopyPresentable {
    func copyString(
        from view: ControllerBackedProtocol,
        stringToCopy: String,
        message: String
    ) {
        UIPasteboard.general.string = stringToCopy

        presentCopyNotification(from: view, message: message)
    }

    func copyString(
        from view: ControllerBackedProtocol,
        stringToCopy: String
    ) {
        copyString(from: view, stringToCopy: stringToCopy, message: String(localized: .Common.copied))
    }

    /// Copies values that must not leak beyond the current device or linger on the pasteboard
    /// (recovery phrase, private keys, etc.). Sets a short expiry and disables Universal Clipboard.
    func copySensitiveString(
        from view: ControllerBackedProtocol,
        stringToCopy: String,
        message: String
    ) {
        let expirationInterval: TimeInterval = 60
        UIPasteboard.general.setItems(
            [[UIPasteboard.typeAutomatic: stringToCopy]],
            options: [
                .localOnly: true,
                .expirationDate: Date(timeIntervalSinceNow: expirationInterval)
            ]
        )

        presentCopyNotification(from: view, message: message)
    }

    func copySensitiveString(
        from view: ControllerBackedProtocol,
        stringToCopy: String
    ) {
        copySensitiveString(from: view, stringToCopy: stringToCopy, message: String(localized: .Common.copied))
    }

    @MainActor
    private func presentCopyNotification(
        from view: ControllerBackedProtocol,
        message: String
    ) {
        guard let notificationView = BottomNotificationFactory.createMessageNotification(for: message) else {
            return
        }

        view.controller.present(
            notificationView.controller,
            animated: true,
            completion: nil
        )
    }
}
