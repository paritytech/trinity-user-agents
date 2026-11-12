import Products
import UIKit
import UIKitExt

final class WalletMainWireframe: WalletMainWireframeProtocol, AlertPresentable {
    private let moduleNavigator: ModuleNavigating
    private let flowState: SPAFlowState
    private let assetDetailsScene: AssetDetailsScene

    init(
        flowState: SPAFlowState,
        assetDetailsScene: AssetDetailsScene,
        moduleNavigator: ModuleNavigating = ModuleNavigator()
    ) {
        self.flowState = flowState
        self.assetDetailsScene = assetDetailsScene
        self.moduleNavigator = moduleNavigator
    }

    func showBalanceCard() {
        let viewModel = assetDetailsScene.viewModel
        moduleNavigator.openWallet { wallet, wasOnScreen in
            let origin = wallet.viewControllers.first as? PocketOrigin
            let screen = AssetDetailsScreenViewController(viewModel: viewModel, origin: origin)
            PocketNavigation.show(screen, on: wallet, animated: wasOnScreen) {}
        }
    }

    func showCollectibles(page: ProductPage) {
        let configuration = SPAConfiguration(
            title: String(localized: .Products.collectibles),
            isRootScreen: false,
            showMoreButton: false,
            page: page
        )
        guard let product = SPAViewFactory.createView(configuration: configuration, flowState: flowState) else {
            return
        }

        moduleNavigator.openWallet { wallet, wasOnScreen in
            let origin = wallet.viewControllers.first as? PocketOrigin
            let screen = CollectiblesViewController(product: product, origin: origin)
            PocketNavigation.show(screen, on: wallet, animated: wasOnScreen) {}
        }
    }

    func showPocketCard(_ card: PocketCardViewModel) {
        guard let pocket = ProductPocketService.current else { return }

        PocketCardOpening(flowState: flowState, navigator: moduleNavigator, pocket: pocket).open(card)
    }

    func confirmPocketCardRemoval(_: PocketCardViewModel, onConfirm: @escaping () -> Void) {
        present(
            viewModel: AlertPresentableViewModel(
                title: String(localized: .Products.pocketCardRemoveConfirmTitle),
                message: String(localized: .Products.pocketCardRemoveConfirmMessage),
                actions: [
                    AlertPresentableAction(
                        title: String(localized: .Products.pocketCardRemove),
                        style: .destructive,
                        handler: onConfirm
                    )
                ],
                closeActionTitle: String(localized: .Common.cancel)
            ),
            style: .alert,
            from: nil
        )
    }
}
