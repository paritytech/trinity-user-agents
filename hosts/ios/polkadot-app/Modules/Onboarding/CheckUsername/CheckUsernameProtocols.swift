import Combine
import Foundation
import UIKitExt

protocol CheckUsernameViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: CheckUsernameViewLayout.ViewModel)
}

@MainActor
protocol CheckUsernamePresenterProtocol: AnyObject {
    func setup()
    func viewDidAppear()
}

protocol CheckUsernameInteractorInputProtocol: AnyObject {
    func onChainUsername() -> AnyPublisher<Username, Error>
    func save(username: Username)
}

@MainActor
protocol CheckUsernameInteractorOutputProtocol: AnyObject {
    func didSaveUsername()
}

@MainActor
protocol CheckUsernameWireframeProtocol: AnyObject {
    func showMainScreen()
    func showClaimUsername()
}
