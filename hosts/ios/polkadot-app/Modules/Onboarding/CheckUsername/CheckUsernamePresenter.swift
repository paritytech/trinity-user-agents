import Foundation
import Combine

final class CheckUsernamePresenter {
    weak var view: CheckUsernameViewProtocol?
    let wireframe: CheckUsernameWireframeProtocol
    let interactor: CheckUsernameInteractorInputProtocol

    private var usernameCancellable: AnyCancellable?

    init(
        interactor: CheckUsernameInteractorInputProtocol,
        wireframe: CheckUsernameWireframeProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
    }
}

extension CheckUsernamePresenter: CheckUsernamePresenterProtocol {
    func setup() {
        view?.didReceive(viewModel: .loading)
    }

    func viewDidAppear() {
        usernameCancellable = interactor.onChainUsername()
            .delayAtLeast(for: 1)
            .receive(on: DispatchQueue.main)
            .sink(receiveCompletion: { [weak wireframe, weak view] completion in
                switch completion {
                case .finished:
                    break
                case .failure(IdentityServiceError.accountNotFound):
                    wireframe?.showClaimUsername()
                case .failure:
                    view?.didReceive(viewModel: .error)
                }
            }, receiveValue: { [weak interactor] username in
                interactor?.save(username: username)
            })
    }
}

extension CheckUsernamePresenter: CheckUsernameInteractorOutputProtocol {
    func didSaveUsername() {
        wireframe.showMainScreen()
    }
}
