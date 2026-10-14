import Foundation
import Foundation_iOS

final class AccountRecoveryPresenter {
    weak var view: AccountRecoveryViewProtocol?

    let wireframe: AccountRecoveryWireframeProtocol
    let interactor: AccountRecoveryInteractorInputProtocol

    private var inputViewModel: InputViewModel?

    init(
        interactor: AccountRecoveryInteractorInputProtocol,
        wireframe: AccountRecoveryWireframeProtocol
    ) {
        self.wireframe = wireframe
        self.interactor = interactor
    }
}

// MARK: - AccountRecoveryPresenterProtocol

extension AccountRecoveryPresenter: AccountRecoveryPresenterProtocol {
    func setup() {
        provideInputViewModel()
    }

    func proceed() {
        interactor.proceed(
            withWords: inputViewModel?.inputHandler.normalizedValue ?? ""
        )
    }
}

// MARK: - AccountRecoveryInteractorOutputProtocol

extension AccountRecoveryPresenter: AccountRecoveryInteractorOutputProtocol {
    func didRestoreWallets() {
        MainActor.assumeIsolated {
            wireframe.didRestoreWallets()
        }
    }

    func didReceiveInvalidMnemonicFormat() {
        showInvalidMnemonicError()
    }

    func didDecideBroken() {
        MainActor.assumeIsolated {
            wireframe.didDecideBroken()
        }
    }

    func authorizeUser(completion: @escaping AuthorizationCompletionBlock) {
        wireframe.authorize(animated: true, retriable: true, with: completion)
    }
}

// MARK: - Private

@MainActor
private extension AccountRecoveryPresenter {
    func provideInputViewModel() {
        let placeholder = String(localized: .accountRecoveryInputPlaceholder)
        let normalizer = MnemonicTextNormalizer()
        let inputHandler = InputHandler(
            value: "",
            maxLength: 250,
            validCharacterSet: .englishMnemonic,
            predicate: .notEmpty,
            normalizer: normalizer
        )
        let viewModel = InputViewModel(
            inputHandler: inputHandler,
            placeholder: placeholder
        )
        inputViewModel = viewModel

        view?.didReceive(inputViewModel: viewModel)
    }

    func showInvalidMnemonicError() {
        guard let view else {
            return
        }

        _ = wireframe.present(
            error: .init(
                title: String(localized: .accountRecoveryErrorTitle),
                message: String(localized: .accountRecoveryErrorMessage)
            ),
            from: view
        )
    }
}
