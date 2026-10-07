import Foundation
import UIKitExt
import PolkadotUI
import ChainRegistry

protocol WalletMainViewProtocol: ControllerBackedProtocol {
    func didReceive(titleViewModel: NetworkStatusTitleView.ViewModel)
}

@MainActor
protocol WalletMainPresenterProtocol: AnyObject {
    func setup()
}

@MainActor
protocol WalletMainWireframeProtocol: AnyObject {}

protocol WalletMainInteractorInputProtocol: AnyObject {
    func setup()
}

@MainActor
protocol WalletMainInteractorOutputProtocol: AnyObject {
    func didReceive(networkStatus: NetworkStatus)
}
