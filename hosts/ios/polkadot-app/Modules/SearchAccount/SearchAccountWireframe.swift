import UIKit
import Coinage
import ChainRegistry
import Products

@MainActor
final class SearchAccountWireframe: SearchAccountWireframeProtocol {
    let coinageServicing: CoinageServicing
    let moduleNavigator: ModuleNavigating

    private lazy var qrScanResultHandler = WalletQRScanResultHandler(
        dsfinvkRouter: W3sDsfinvkRouter.createDefault(coinageService: coinageServicing)
    )

    init(coinageServicing: CoinageServicing, moduleNavigator: ModuleNavigating = ModuleNavigator()) {
        self.coinageServicing = coinageServicing
        self.moduleNavigator = moduleNavigator
    }

    func showQRScan(from view: SearchAccountViewProtocol?) {
        showWalletQRScan(from: view, resultHandler: qrScanResultHandler)
    }

    func showTransfer(
        from view: SearchAccountViewProtocol?,
        recipient: RecipientModel,
        chainAsset: ChainAsset
    ) {
        guard
            let destination = TransferAmountViewFactory.createTransfer(
                for: chainAsset,
                recipient: recipient,
                coinageService: coinageServicing
            )
        else {
            return
        }

        view?.controller.navigationController?.pushViewController(destination.controller, animated: true)
    }

    func showChat(_ model: ChatOpenModel) {
        moduleNavigator.openChat(model)
    }

    func showProduct(page: ProductPage) {
        moduleNavigator.openProduct(page: page)
    }

    func close(from view: SearchAccountViewProtocol?) {
        view?.controller.navigationController?.popViewController(animated: false)
    }
}
