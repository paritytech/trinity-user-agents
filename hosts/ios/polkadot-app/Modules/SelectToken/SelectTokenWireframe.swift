import Foundation
import ChainRegistry

@MainActor
final class SelectTokenWireframe: SelectTokenWireframeProtocol {
    let context: WalletFlowContextProtocol

    init(context: WalletFlowContextProtocol) {
        self.context = context
    }

    func proceed(from view: SelectTokenViewProtocol?, chainAsset: ChainAsset) {
        guard let destination = DepositViewFactory.createView(
            chainAsset: chainAsset,
            depositService: context.depositService,
            completion: { [weak view] in
                view?.controller.navigationController?.popToRootViewController(animated: true)
            }
        ) else {
            return
        }
        let navigation = AppNavigationController(rootViewController: destination.controller)
        navigation.barSettings = .defaultSettings.bySettingCloseButton(false)

        view?.controller.present(navigation, animated: true)
    }

    func proceedToFiatOnRamp(from view: SelectTokenViewProtocol?) {
        guard let destination = FiatOnRampViewFactory.createView(context: context) else {
            return
        }
        if let navigationController = view?.controller.navigationController {
            navigationController.pushViewController(destination.controller, animated: true)
        } else {
            let navigation = AppNavigationController(rootViewController: destination.controller)
            navigation.barSettings = .defaultSettings.bySettingCloseButton(false)
            view?.controller.present(navigation, animated: true)
        }
    }
}
