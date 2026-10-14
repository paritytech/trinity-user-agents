import UIKit
import UIKitExt

@MainActor
protocol ApplicationSettingsPresentable: AnyObject {
    @discardableResult
    func openApplicationSettings() -> Bool
}

extension ApplicationSettingsPresentable {
    func askOpenApplicationSettings(
        with message: String,
        title: String?,
        from view: ControllerBackedProtocol?,
        completion: ((ApplicationSettingsAlertResult) -> Void)? = nil
    ) {
        var currentController = view?.controller

        if currentController == nil {
            currentController = UIApplication.shared.delegate?.window??.rootViewController
        }

        guard
            let controller = currentController,
            controller.viewIfLoaded?.window != nil,
            controller.presentedViewController == nil
        else {
            completion?(.notPresented)
            return
        }

        let completion = ApplicationSettingsPresentationCompletion(completion)

        let alert = UIAlertController(title: title, message: message, preferredStyle: .alert)

        let closeTitle = String(localized: .Common.notNow)
        let closeAction = UIAlertAction(title: closeTitle, style: .cancel) { _ in
            completion.complete(.closed)
        }

        let settingsTitle = String(localized: .Common.openSettings)
        let settingsAction = UIAlertAction(title: settingsTitle, style: .default) { [weak self] _ in
            let didOpenSettings = self?.openApplicationSettings() == true
            completion.complete(didOpenSettings ? .openedSettings : .closed)
        }

        alert.addAction(closeAction)
        alert.addAction(settingsAction)

        controller.present(alert, animated: true, completion: nil)
    }

    @discardableResult
    func openApplicationSettings() -> Bool {
        guard
            let url = URL(string: UIApplication.openSettingsURLString),
            UIApplication.shared.canOpenURL(url)
        else {
            return false
        }
        UIApplication.shared.open(url, options: [:], completionHandler: nil)
        return true
    }
}

enum ApplicationSettingsAlertResult {
    case notPresented
    case openedSettings
    case closed
    case undefined
}

private final class ApplicationSettingsPresentationCompletion {
    private var completion: ((ApplicationSettingsAlertResult) -> Void)?

    init(_ completion: ((ApplicationSettingsAlertResult) -> Void)?) {
        self.completion = completion
    }

    func complete(_ result: ApplicationSettingsAlertResult) {
        completion?(result)
        completion = nil
    }

    deinit {
        completion?(.undefined)
    }
}
