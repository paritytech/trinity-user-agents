import Foundation
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

        let view = SecretPhraseMnemonicViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
