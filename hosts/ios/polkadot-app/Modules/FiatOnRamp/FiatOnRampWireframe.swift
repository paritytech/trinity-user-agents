import UIKit

final class FiatOnRampWireframe: FiatOnRampWireframeProtocol {
    let context: WalletFlowContextProtocol

    init(context: WalletFlowContextProtocol) {
        self.context = context
    }

    func showProviders(
        from view: FiatOnRampViewProtocol?,
        amount: Decimal,
        purchaseLimit: FiatOnrampFiatPurchaseLimit?
    ) {
        guard let view else {
            return
        }

        guard let destination = FiatOnRampProviderViewFactory.createView(
            context: context,
            amount: amount,
            purchaseLimit: purchaseLimit
        ) else {
            return
        }

        if let navigationController = view.controller.navigationController {
            navigationController.pushViewController(destination.controller, animated: true)
        } else {
            view.controller.present(destination.controller, animated: true)
        }
    }
}
