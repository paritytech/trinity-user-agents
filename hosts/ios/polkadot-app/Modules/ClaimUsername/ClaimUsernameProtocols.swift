import Foundation_iOS
import UIKitExt
import PolkadotUI

protocol ClaimUsernameViewProtocol: ControllerValidationResultPresentable {
    func didReceive(viewModel: ClaimUsernameContentViewModel)
    func didReceive(usernameInputViewModel: InputViewModelProtocol)
    func didReceive(digitsOptions: [String])
    func didReceive(digitsState: DigitsFieldState)
    func didStartLoading()
    func didStopLoading()
    func userInteraction(enabled: Bool)
    func setAccountCreationInProgress(_ inProgress: Bool)
}

@MainActor
protocol ClaimUsernamePresenterProtocol: AnyObject {
    func setup()
    func update(from viewModel: InputViewModelProtocol)
    func updateDigits(_ value: String)
    func resolveError()
    func confirm()
    func recover()
}

protocol ClaimUsernameInteractorInputProtocol: AnyObject {
    var metadata: UsernameMetadata { get }

    func check(username: Username) async throws -> UsernameAvailableType
    func claim(username: Username) async throws -> Username
    func save(username: Username) async
}

@MainActor
protocol ClaimUsernameInteractorOutputProtocol: AnyObject {
    func didSaveUsername()
}

@MainActor
protocol ClaimLiteUsernameInteractorOutputProtocol: ClaimUsernameInteractorOutputProtocol {
    func authorizeUser(completion: @escaping AuthorizationCompletionBlock)
    func didChangeAccountCreation(inProgress: Bool)
}

@MainActor
protocol ClaimUsernameWireframeProtocol:
    AlertPresentable,
    UsernameValidationErrorPresentable,
    AuthorizationPresentable,
    ErrorPresentable {
    func finishFlow(from view: ControllerBackedProtocol?)
    func showRecovery(from view: ControllerBackedProtocol?)
}
