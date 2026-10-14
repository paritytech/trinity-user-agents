import Foundation
import NovaCrypto

final class SecretPhraseMnemonicPresenter {
    // MARK: Properties

    weak var view: SecretPhraseMnemonicViewProtocol?
    let wireframe: SecretPhraseMnemonicWireframeProtocol
    let interactor: SecretPhraseMnemonicInteractorInputProtocol

    // MARK: Initial methods

    init(
        interactor: SecretPhraseMnemonicInteractorInputProtocol,
        wireframe: SecretPhraseMnemonicWireframeProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
    }
}

// MARK: - SecretPhraseMnemonicPresenterProtocol

extension SecretPhraseMnemonicPresenter: SecretPhraseMnemonicPresenterProtocol {
    func onShowMnemonic() {
        view?.showMnemonic(true)
    }

    func onHideMnemonic() {
        view?.showMnemonic(false)
    }

    func setup() {
        interactor.requestMnemonicData()
    }

    func copyDataInBuffer(_ data: String) {
        guard let view else {
            return
        }

        wireframe.copySensitiveString(from: view, stringToCopy: data)
    }
}

// MARK: - SecretPhraseMnemonicInteractorOutputProtocol

extension SecretPhraseMnemonicPresenter: SecretPhraseMnemonicInteractorOutputProtocol {
    func didReceiveMnemonic(_ mnemonic: any IRMnemonicProtocol) {
        var cells = [SecretPhraseMnemonicViewModel.Cell]()
        cells.reserveCapacity(Int(mnemonic.numberOfWords()))
        cells.append(contentsOf: mnemonic.allWords().enumerated().map { .phrase($0.offset + 1, $0.element) })
        let viewModel = SecretPhraseMnemonicViewModel(cells: cells)
        view?.updateViewModel(viewModel)
    }
}
