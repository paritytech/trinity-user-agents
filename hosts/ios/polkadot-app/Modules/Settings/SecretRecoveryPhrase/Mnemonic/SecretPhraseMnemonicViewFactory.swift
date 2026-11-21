import Foundation
import FoundationExt
import Keystore_iOS
import NovaCrypto
import KeyDerivation

@MainActor
enum SecretPhraseMnemonicViewFactory {
    static func createView() -> SecretPhraseMnemonicViewProtocol? {
        let interactor = SecretPhraseMnemonicInteractor(
            entropyManager: RootEntropyManager.shared,
            mnemonicGenerator: IRMnemonicCreator(),
            logger: Logger.shared
        )
        let wireframe = SecretPhraseMnemonicWireframe()

        let presenter = SecretPhraseMnemonicPresenter(interactor: interactor, wireframe: wireframe)

        let view = SecretPhraseMnemonicViewController(
            presenter: presenter,
            screenCaptureProtector: ScreenCaptureProtector(appStateStreamFactory: ApplicationStateStreamFactory())
        )

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
