import UIKit
import UIKitExt

@MainActor
final class ChatCallWireframe: ChatCallWireframeProtocol, AlertPresentable, ApplicationSettingsPresentable {
    func close(from view: ChatCallViewProtocol?) {
        view?.controller.dismiss(animated: true)
    }

    func presentMicrophoneAccessRequired(from view: ChatCallViewProtocol?) {
        presentAccessRequired(
            title: String(localized: .chatCallMicAccessTitle),
            message: String(localized: .chatCallMicAccessMessage),
            from: view
        )
    }

    func presentCameraAccessRequired(from view: ChatCallViewProtocol?) {
        presentAccessRequired(
            title: String(localized: .chatCallCameraAccessTitle),
            message: String(localized: .chatCallCameraAccessMessage),
            from: view
        )
    }

    func presentVideoCaptureFailed(from view: ChatCallViewProtocol?) {
        let viewModel = AlertPresentableViewModel(
            title: String(localized: .chatCallVideoUnavailableTitle),
            message: String(localized: .chatCallVideoUnavailableMessage),
            actions: [],
            closeActionTitle: String(localized: .Common.gotIt)
        )

        present(viewModel: viewModel, style: .alert, from: view)
    }
}

private extension ChatCallWireframe {
    func presentAccessRequired(title: String, message: String, from view: ChatCallViewProtocol?) {
        let viewModel = AlertPresentableViewModel(
            title: title,
            message: message,
            actions: [
                AlertPresentableAction(title: String(localized: .Common.openSettings)) { [weak self] in
                    self?.openApplicationSettings()
                }
            ],
            closeActionTitle: String(localized: .Common.notNow)
        )

        present(viewModel: viewModel, style: .alert, from: view)
    }
}
