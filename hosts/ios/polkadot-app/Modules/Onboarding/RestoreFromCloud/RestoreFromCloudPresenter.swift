import Foundation

final class RestoreFromCloudPresenter {
    weak var view: RestoreFromCloudViewProtocol?
    let wireframe: RestoreFromCloudWireframeProtocol
    let interactor: RestoreFromCloudInteractorInputProtocol

    init(
        interactor: RestoreFromCloudInteractorInputProtocol,
        wireframe: RestoreFromCloudWireframeProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
    }
}

extension RestoreFromCloudPresenter: RestoreFromCloudPresenterProtocol {
    func setup() {
        provideViewModel(.idle)
    }

    func viewDidAppear() {
        interactor.restoreWallets()
    }
}

extension RestoreFromCloudPresenter: RestoreFromCloudInteractorOutputProtocol {
    func didReceiveInProgress(_ value: Bool) {
        provideViewModel(value ? .inProgress : .idle)
    }

    func didFailAuthorization() {
        provideViewModel(.authFailed)
    }

    func didRestoreWallets() {
        MainActor.assumeIsolated {
            wireframe.observer.didRestoreWallets()
        }
    }

    func didDecideBroken() {
        MainActor.assumeIsolated {
            wireframe.observer.didDecideBroken()
        }
    }

    func authorizeUser(completion: @escaping AuthorizationCompletionBlock) {
        wireframe.authorize(animated: true, retriable: true, with: completion)
    }
}

private extension RestoreFromCloudPresenter {
    func provideViewModel(_ viewModel: RestoreFromCloudViewLayout.ViewModel) {
        view?.didReceive(viewModel: viewModel)
    }
}
