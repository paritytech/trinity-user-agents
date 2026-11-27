import Foundation
import Foundation_iOS
import FoundationExt
import NovaCrypto
import Keystore_iOS
import KeyDerivation

@MainActor
enum AccountRecoveryViewFactory {
    static func createView(
        observer: RootStateObserving
    ) -> AccountRecoveryViewProtocol? {
        let walletSetupManager = WalletSetupManager(
            mnemonicGenerator: IRMnemonicCreator(),
            mnemonicBackupHelper: MnemonicBackupHelper(),
            entropyManager: RootEntropyManager.shared,
            logger: Logger.shared
        )

        let interactor = AccountRecoveryInteractor(
            mnemonicGenerator: IRMnemonicCreator(),
            walletSetupManager: walletSetupManager,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            logger: Logger.shared
        )

        let wireframe = AccountRecoveryWireframe(observer: observer)

        let presenter = AccountRecoveryPresenter(
            interactor: interactor,
            wireframe: wireframe
        )

        let view = AccountRecoveryViewController(
            presenter: presenter,
            screenCaptureProtector: ScreenCaptureProtector(appStateStreamFactory: ApplicationStateStreamFactory())
        )

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
