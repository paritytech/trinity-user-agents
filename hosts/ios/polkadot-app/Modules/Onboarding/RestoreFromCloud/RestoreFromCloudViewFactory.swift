import Foundation
import Keystore_iOS
import NovaCrypto
import KeyDerivation

@MainActor
enum RestoreFromCloudViewFactory {
    static func createView(with observer: RootStateObserving) -> RestoreFromCloudViewProtocol? {
        let walletSetupManager = WalletSetupManager(
            mnemonicGenerator: IRMnemonicCreator(),
            mnemonicBackupHelper: MnemonicBackupHelper(),
            entropyManager: RootEntropyManager.shared,
            logger: Logger.shared
        )

        let interactor = RestoreFromCloudInteractor(
            walletSetupManager: walletSetupManager,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            logger: Logger.shared
        )
        let wireframe = RestoreFromCloudWireframe(observer: observer)
        let presenter = RestoreFromCloudPresenter(interactor: interactor, wireframe: wireframe)
        let view = RestoreFromCloudViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
