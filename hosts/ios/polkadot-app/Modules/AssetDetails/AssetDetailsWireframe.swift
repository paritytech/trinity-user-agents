import UIKit
import Foundation_iOS
import UIKitExt
import ChainRegistry

@MainActor
final class AssetDetailsWireframe: AssetDetailsWireframeProtocol {
    let context: WalletFlowContextProtocol

    private let moduleNavigator: ModuleNavigating

    init(context: WalletFlowContextProtocol, moduleNavigator: ModuleNavigating = ModuleNavigator()) {
        self.context = context
        self.moduleNavigator = moduleNavigator
    }

    func showBackupSettings() {
        moduleNavigator.openBackupSettings()
    }

    func showTransfer(from view: ControllerBackedProtocol?, chainAsset: ChainAsset) {
        guard let searchView = SearchAccountViewFactory.createView(
            for: chainAsset,
            coinageServicing: context.coinageService
        ) else {
            return
        }

        view?.controller.navigationController?.pushViewController(
            searchView.controller,
            animated: true
        )
    }

    func showAddTokens(from view: (any ControllerBackedProtocol)?) {
        guard
            let destination = SelectTokenViewFactory.createView(
                supportedTokens: AppConfig.Assets.fundingAssets,
                context: context
            ) else {
            return
        }
        view?.controller.navigationController?.pushViewController(
            destination.controller,
            animated: true
        )
    }

    func showFundingHistory() {
        FundingActivityCenter.shared.showHistory()
    }
}
