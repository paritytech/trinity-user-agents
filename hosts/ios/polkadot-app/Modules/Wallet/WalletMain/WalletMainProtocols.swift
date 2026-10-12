import Foundation
import UIKitExt
import PolkadotUI
import ChainRegistry

protocol WalletMainViewProtocol: ControllerBackedProtocol {
    func didReceive(isCollectiblesAvailable: Bool)
    func didReceive(pocketCards: [PocketCardViewModel])
    func didReceive(titleViewModel: NetworkStatusTitleView.ViewModel)
}

@MainActor
protocol WalletMainPresenterProtocol: AnyObject {
    func setup()
    func showCollectibles()
    func showPocketCard(_ card: PocketCardViewModel)
    func removePocketCard(_ card: PocketCardViewModel)
}

@MainActor
protocol WalletMainWireframeProtocol: AnyObject {
    func showCollectibles(from view: WalletMainViewProtocol?, url: URL)
    func showPocketCard(_ card: PocketCardViewModel)
    func confirmPocketCardRemoval(_ card: PocketCardViewModel, onConfirm: @escaping () -> Void)
}

protocol WalletMainInteractorInputProtocol: AnyObject {
    func setup()
    func removePocketCard(_ card: PocketCardViewModel)
}

@MainActor
protocol WalletMainInteractorOutputProtocol: AnyObject {
    func didReceiveCollectibles(url: URL?)
    func didReceive(networkStatus: NetworkStatus)
    func didReceive(pocketCards: [PocketCardViewModel])
}
