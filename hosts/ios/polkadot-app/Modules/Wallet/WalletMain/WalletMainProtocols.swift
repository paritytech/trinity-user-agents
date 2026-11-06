import Foundation
import UIKitExt
import PolkadotUI
import ChainRegistry
import Products

protocol WalletMainViewProtocol: ControllerBackedProtocol, PocketCardOrigin {
    func didReceive(isCollectiblesAvailable: Bool)
    func didReceive(pocketCards: [PocketCardViewModel])
}

@MainActor
protocol WalletMainPresenterProtocol: AnyObject {
    func setup()
    func showCollectibles()
    func showPocketCard(_ card: PocketCardViewModel)
    func removePocketCard(_ card: PocketCardViewModel)
    func showFundingHistory()
}

@MainActor
protocol WalletMainWireframeProtocol: AnyObject {
    func showProduct(page: ProductPage)
    func showPocketCard(_ card: PocketCardViewModel)
    func confirmPocketCardRemoval(_ card: PocketCardViewModel, onConfirm: @escaping () -> Void)
    func showFundingHistory()
}

protocol WalletMainInteractorInputProtocol: AnyObject {
    func setup()
    func removePocketCard(_ card: PocketCardViewModel)
}

@MainActor
protocol WalletMainInteractorOutputProtocol: AnyObject {
    func didReceiveCollectibles(page: ProductPage)
    func didReceive(pocketCards: [PocketCardViewModel])
}
