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

    func showPocketCard(_ card: PocketCardViewModel) {
        guard let pocket = ProductPocketService.current else { return }

        PocketCardOpening.open(card, flowState: flowState, navigator: moduleNavigator, pocket: pocket)
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
