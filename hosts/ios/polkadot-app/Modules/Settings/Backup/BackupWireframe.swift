import UIKit
import UIKit_iOS
import Foundation_iOS

@MainActor
final class BackupWireframe: BackupWireframeProtocol {
    func showEnableCloud(from view: BackupViewProtocol?) {
        let controller = EnableCloudViewController()
        BottomSheetViewFacade.setupBottomSheet(from: controller, preferredHeight: nil)
        view?.controller.present(controller, animated: true)
    }

    func showWarning(from view: (any BackupViewProtocol)?, action: @escaping () -> Void) {
        guard let destination = RecoveryWarningViewFactory.createView(action: action) else {
            return
        }

        view?.controller.present(destination.controller, animated: true)
    }

    func openSecretRecoveryPhase(from view: (any BackupViewProtocol)?) {
        guard let mnemonicView = SecretPhraseMnemonicViewFactory.createView() else {
            return
        }
        let navigationController = view?.controller.navigationController as? AppNavigationController
        navigationController?.pushViewController(
            mnemonicView.controller,
            animated: true
        )
    }
}
