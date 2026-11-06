import Products
import UIKit
import UIKitExt

final class WalletMainWireframe: WalletMainWireframeProtocol, AlertPresentable {
    private let moduleNavigator: ModuleNavigating
    private let flowState: SPAFlowState

    init(
        flowState: SPAFlowState,
        moduleNavigator: ModuleNavigating = ModuleNavigator()
    ) {
        self.flowState = flowState
        self.moduleNavigator = moduleNavigator
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

        let screen = CollectiblesViewController(product: product)
        moduleNavigator.openWallet { wallet, wasOnScreen in
            wallet.pushViewController(screen, animated: wasOnScreen)
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
