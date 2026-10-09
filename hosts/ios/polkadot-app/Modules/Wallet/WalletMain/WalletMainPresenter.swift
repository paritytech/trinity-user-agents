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

    func showPocketCard(_ card: PocketCardViewModel) {
        wireframe.showPocketCard(card)
    }

    /// Confirmed first: a long press is easy to make by accident, and the card
    /// cannot be put back without the product offering it again.
    func removePocketCard(_ card: PocketCardViewModel) {
        wireframe.confirmPocketCardRemoval(card) { [weak self] in
            self?.interactor.removePocketCard(card)
        }
    }
}

extension WalletMainPresenter: WalletMainInteractorOutputProtocol {
    func didReceive(pocketCards: [PocketCardViewModel]) {
        view?.didReceive(pocketCards: pocketCards)
    }

    func didReceive(networkStatus: NetworkStatus) {
        let titleViewModel = titleViewModelFactory.createTitleViewModel(for: networkStatus)
        view?.didReceive(titleViewModel: titleViewModel)
    }
}
