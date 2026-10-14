import Foundation
import PolkadotUI
import UIKitExt

protocol FiatOnRampViewProtocol: ControllerBackedProtocol {
    func didReceive(quickAmounts: [FiatOnRampQuickAmountViewModel])
    func didReceive(amount: Int?)
    func didReceive(amountError: String?)
}

@MainActor
protocol FiatOnRampPresenterProtocol: AnyObject {
    func setup()
    func onAmountChanged(_ amount: Int?)
    func onContinue(amount: Int?)
    func onSelectQuickAmount(_ quickAmount: FiatOnRampQuickAmountViewModel)
}

protocol FiatOnRampInteractorInputProtocol: AnyObject {
    func setup()
}

@MainActor
protocol FiatOnRampInteractorOutputProtocol: AnyObject {
    func didReceive(purchaseLimit: FiatOnrampFiatPurchaseLimit?)
}

@MainActor
protocol FiatOnRampWireframeProtocol: AnyObject {
    func showProviders(
        from view: FiatOnRampViewProtocol?,
        amount: Decimal,
        purchaseLimit: FiatOnrampFiatPurchaseLimit?
    )
}
