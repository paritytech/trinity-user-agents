import UIKit
import UIKitExt

@MainActor
final class ChatCallWireframe: ChatCallWireframeProtocol, AlertPresentable, ApplicationSettingsPresentable {
    func close(from view: ChatCallViewProtocol?) {
        view?.controller.dismiss(animated: true)
    }

    func presentMicrophoneAccessRequired(from view: ChatCallViewProtocol?) {
        let viewModel = AlertPresentableViewModel(
            title: String(localized: .chatCallMicAccessTitle),
            message: String(localized: .chatCallMicAccessMessage),
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
