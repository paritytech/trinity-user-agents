import Foundation
import ChainRegistry

@MainActor
final class WalletMainPresenter {
    weak var view: WalletMainViewProtocol?
    let wireframe: WalletMainWireframeProtocol
    let interactor: WalletMainInteractorInputProtocol
    let titleViewModelFactory: NetworkStatusTitleViewModelMaking

    init(
        interactor: WalletMainInteractorInputProtocol,
        wireframe: WalletMainWireframeProtocol,
        titleViewModelFactory: NetworkStatusTitleViewModelMaking
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.titleViewModelFactory = titleViewModelFactory
    }
}

extension WalletMainPresenter: WalletMainPresenterProtocol {
    func setup() {
        didReceive(networkStatus: .connected)
        interactor.setup()
    }
}

extension WalletMainPresenter: WalletMainInteractorOutputProtocol {
    func didReceive(networkStatus: NetworkStatus) {
        let titleViewModel = titleViewModelFactory.createTitleViewModel(for: networkStatus)
        view?.didReceive(titleViewModel: titleViewModel)
    }
}
