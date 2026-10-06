import Foundation
import Foundation_iOS
import UIKitExt
import ChainRegistry
import SubstrateSdk

protocol SearchAccountViewProtocol: ControllerBackedProtocol {
    var viewModel: SearchAccountViewModel { get }
    func didReceive(_ viewModel: SearchAccountViewModel)
    func applyData(_ viewModel: SearchAccountViewModel)
    func didReceive(status: SearchAccountViewModel.Status)
}

@MainActor
protocol SearchAccountPresenterProtocol: AnyObject {
    func viewDidLoad()
    func scanQRCode()
    func didEndEditingInput(_ input: String?)
    func searchAccount(_ account: String?)
    func selectAccount(_ cellType: SearchAccountViewController.Cell)
}

typealias SearchAccountSearchState = SearchRunner.State<SearchAccountResult>

protocol SearchAccountInteractorInputProtocol: AnyObject {
    func setup()
    func searchAccount(for input: String?)
    func resolveChat(for address: AccountAddress)
}

@MainActor
protocol SearchAccountInteractorOutputProtocol: AnyObject {
    func didReceive(searchState: SearchAccountSearchState)
    func didResolveChat(_ model: ChatOpenModel)
    func didReceiveSearchError(message: String?)
}

@MainActor
protocol SearchAccountWireframeProtocol: AnyObject, WalletQRScanPresentable, AlertPresentable {
    func showQRScan(from view: SearchAccountViewProtocol?)
    func showTransfer(
        from view: SearchAccountViewProtocol?,
        recipient: RecipientModel,
        chainAsset: ChainAsset
    )
    func showChat(_ model: ChatOpenModel)
}
