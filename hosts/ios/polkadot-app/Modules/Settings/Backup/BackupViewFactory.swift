import Foundation
import Keystore_iOS
import KeyDerivation

@MainActor
enum BackupViewFactory {
    static func createView() -> BackupViewProtocol? {
        let logger = Logger.shared
        let interactor = BackupInteractor(
            cloudKeychain: SynchronizableKeychain(),
            entropyManager: RootEntropyManager.shared,
            logger: logger
        )
        let wireframe = BackupWireframe()

        let presenter = BackupPresenter(
            interactor: interactor,
            wireframe: wireframe,
            logger: logger
        )

        let view = BackupViewController(presenter: presenter)
        view.controller.navigationItem.largeTitleDisplayMode = .never
        view.controller.hidesBottomBarWhenPushed = true

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
