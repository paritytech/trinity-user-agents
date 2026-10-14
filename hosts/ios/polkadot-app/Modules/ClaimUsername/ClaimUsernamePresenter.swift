import Foundation
import Foundation_iOS
import PolkadotUI

@MainActor
final class ClaimUsernamePresenter {
    weak var view: ClaimUsernameViewProtocol?
    let wireframe: ClaimUsernameWireframeProtocol
    let interactor: ClaimUsernameInteractorInputProtocol
    let validationFactory: UsernameValidationFactoryProtocol
    let logger: LoggerProtocol
    let viewModelProvider: ClaimUsernameViewModelProviding

    private var partialNormalizedUsername: String {
        usernameViewModel?.inputHandler.normalizedValue ?? ""
    }

    private var usernameCheckResult: UsernameAvailableType?
    private var usernameViewModel: InputViewModelProtocol?

    private var selectedDigits: Int?
    private var digitsFieldState: DigitsFieldState = .hidden

    private var claimTask: Task<Void, Never>?
    private var checkTask: Task<Void, Never>?

    private let prefilledUsername: Username?

    init(
        interactor: ClaimUsernameInteractorInputProtocol,
        wireframe: ClaimUsernameWireframeProtocol,
        validationFactory: UsernameValidationFactoryProtocol,
        viewModelProvider: ClaimUsernameViewModelProviding,
        prefilledUsername: Username?,
        logger: LoggerProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.validationFactory = validationFactory
        self.logger = logger
        self.prefilledUsername = prefilledUsername
        self.viewModelProvider = viewModelProvider
    }
}

extension ClaimUsernamePresenter {
    private func validateUsername() {
        let metadata = interactor.metadata

        DataValidationRunner(
            validators: [
                validationFactory.notViolatingMinLength(
                    for: partialNormalizedUsername,
                    minLength: metadata.minLength
                ),
                validationFactory.notViolatingMaxLength(
                    for: partialNormalizedUsername,
                    maxLength: metadata.maxLength
                ),
                validationFactory.notValid(from: usernameCheckResult),
                validationFactory.notTaken(from: usernameCheckResult)
            ]
        ).runValidation { [weak self] in
            self?.view?.didReceiveValidation(result: .valid)
        }
    }

    private func resetDigitsState() {
        selectedDigits = nil
        digitsFieldState = .hidden
        view?.didReceive(digitsState: .hidden)
    }

    private func provideInputViewModel(shouldPrefill: Bool) {
        let viewModel = InputViewModel.createUsernameInputViewModel(
            for: shouldPrefill ? prefilledUsername : nil,
            metadata: interactor.metadata
        )
        usernameViewModel = viewModel
        view?.didReceive(usernameInputViewModel: viewModel)

        if shouldPrefill, prefilledUsername != nil {
            update(from: viewModel)
        }
    }

    private func doUsernameCheckUpdateIfPossible() {
        let wasVisible = digitsFieldState != .hidden
        resetDigitsState()

        guard
            let usernameViewModel,
            usernameViewModel.inputHandler.completed
        else {
            view?.didStopLoading()
            usernameCheckResult = .invalid
            checkTask?.cancel()
            checkTask = nil
            validateUsername()
            return
        }

        if wasVisible {
            digitsFieldState = .loading
            view?.didReceive(digitsState: .loading)
        }

        view?.didStartLoading()

        let username = Username(value: partialNormalizedUsername)
        checkTask?.cancel()
        checkTask = Task { [weak self] in
            guard let self else { return }
            let result: UsernameAvailableType
            do {
                result = try await interactor.check(username: username)
            } catch {
                result = .error(error.localizedDescription)
            }
            guard !Task.isCancelled else { return }
            didCompleteCheck(for: username, result: result)
        }
    }
}

extension ClaimUsernamePresenter: ClaimUsernamePresenterProtocol {
    func setup() {
        provideViewModel()
        provideInputViewModel(shouldPrefill: true)
        validateUsername()
    }

    func update(from _: InputViewModelProtocol) {
        doUsernameCheckUpdateIfPossible()
    }

    func updateDigits(_ value: String) {
        selectedDigits = Int(value)
        view?.didReceive(digitsState: .shown)
        validateUsername()
    }

    func resolveError() {
        switch usernameCheckResult {
        case .taken,
             .invalid:
            usernameCheckResult = nil
            resetDigitsState()
            provideInputViewModel(shouldPrefill: false)
            validateUsername()
        case .error:
            doUsernameCheckUpdateIfPossible()
        case .available,
             nil:
            break
        }
    }

    func confirm() {
        guard claimTask == nil else {
            return
        }

        guard
            let usernameViewModel,
            usernameViewModel.inputHandler.completed
        else {
            return
        }

        let username =
            if let selectedDigits {
                Username(name: partialNormalizedUsername, digits: selectedDigits)
            } else {
                Username(value: partialNormalizedUsername)
            }

        view?.userInteraction(enabled: false)
        view?.didStartLoading()
        claimTask = Task { [weak self] in
            guard let self else { return }
            do {
                let claimed = try await interactor.claim(username: username)
                await didReceive(username: claimed)
            } catch {
                didReceive(error: .claimFailed(error))
            }
        }
    }

    func recover() {
        wireframe.showRecovery(from: view)
    }
}

extension ClaimUsernamePresenter: ClaimLiteUsernameInteractorOutputProtocol {
    func didSaveUsername() {
        wireframe.finishFlow(from: view)
    }

    func authorizeUser(completion: @escaping AuthorizationCompletionBlock) {
        wireframe.authorize(animated: true, retriable: true, with: completion)
    }

    func didChangeAccountCreation(inProgress: Bool) {
        view?.setAccountCreationInProgress(inProgress)
        view?.userInteraction(enabled: !inProgress)
    }
}

private extension ClaimUsernamePresenter {
    func provideViewModel() {
        view?.didReceive(viewModel: viewModelProvider.viewModel())
    }

    func didReceive(username: Username) async {
        logger.debug("Username: \(username)")
        await interactor.save(username: username)
    }

    func didCompleteCheck(for username: Username, result: UsernameAvailableType) {
        logger.debug("Check result: \(result) for username: \(username.value)")

        view?.didStopLoading()

        guard
            partialNormalizedUsername == username.partialUsername
        else { return }

        usernameCheckResult = result

        switch result {
        case let .available(digits):
            if let first = digits.first {
                selectedDigits = first
                digitsFieldState = .shown
                view?.didReceive(digitsOptions: digits.map { String(format: "%02d", $0) })
                view?.didReceive(digitsState: .shown)
            }
            validateUsername()
        case .taken where digitsFieldState == .loading,
             .invalid where digitsFieldState == .loading:
            digitsFieldState = .hidden
            view?.didReceive(digitsState: .hidden)
            validateUsername()
        case .taken,
             .invalid:
            validateUsername()
        case .error:
            view?.didReceiveValidation(
                result: .issue(
                    message: "",
                    context: UsernameValidationContext.usernameCheckFailed
                )
            )
        }
    }

    func didReceive(error: ClaimUsernameInteractorError) {
        logger.error("Error: \(error)")
        claimTask = nil
        view?.didStopLoading()
        view?.userInteraction(enabled: true)

        switch error {
        case .claimTimeout:
            break
        case let .claimFailed(remoteError):
            guard !wireframe.present(error: remoteError, from: view) else {
                return
            }
            wireframe.present(
                message: String(localized: .claimUsernameActionError),
                title: String(localized: .Common.error),
                closeAction: String(localized: .Common.close),
                from: view
            )
        }
    }
}
