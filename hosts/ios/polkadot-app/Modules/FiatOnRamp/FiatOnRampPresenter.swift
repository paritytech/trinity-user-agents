import Foundation
import PolkadotUI

final class FiatOnRampPresenter {
    weak var view: FiatOnRampViewProtocol?
    let interactor: FiatOnRampInteractorInputProtocol
    let wireframe: FiatOnRampWireframeProtocol
    private let quickAmountValues: [Int] = [
        50,
        100,
        200
    ]
    private var amount: Int?
    private var amountError: String?
    private var purchaseLimit: FiatOnrampFiatPurchaseLimit?

    private lazy var limitFormatter: NumberFormatter = {
        let formatter = NumberFormatter()
        formatter.numberStyle = .currency
        formatter.minimumFractionDigits = 0
        formatter.maximumFractionDigits = 2
        formatter.locale = Locale.autoupdatingCurrent
        return formatter
    }()

    init(
        interactor: FiatOnRampInteractorInputProtocol,
        wireframe: FiatOnRampWireframeProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
    }
}

extension FiatOnRampPresenter: FiatOnRampPresenterProtocol {
    func setup() {
        provideQuickAmounts()
        interactor.setup()
    }

    func onAmountChanged(_ amount: Int?) {
        updateAmount(amount)
    }

    func onContinue(amount: Int?) {
        guard let amount, amount > 0, amountError == nil else {
            return
        }

        wireframe.showProviders(
            from: view,
            amount: Decimal(amount),
            purchaseLimit: purchaseLimit
        )
    }

    func onSelectQuickAmount(_ quickAmount: FiatOnRampQuickAmountViewModel) {
        updateAmount(quickAmount.value)
    }
}

extension FiatOnRampPresenter: FiatOnRampInteractorOutputProtocol {
    func didReceive(purchaseLimit: FiatOnrampFiatPurchaseLimit?) {
        self.purchaseLimit = purchaseLimit
        updateAmountError()
    }
}

private extension FiatOnRampPresenter {
    func provideQuickAmounts() {
        let amounts = quickAmountValues.map { value in
            FiatOnRampQuickAmountViewModel(
                id: String(value),
                value: value,
                title: "$\(value)"
            )
        }
        view?.didReceive(quickAmounts: amounts)
    }

    func updateAmount(_ amount: Int?) {
        self.amount = amount
        view?.didReceive(amount: amount)
        updateAmountError()
    }

    func updateAmountError() {
        let error = makeAmountError(for: amount)
        amountError = error
        view?.didReceive(amountError: error)
    }

    func makeAmountError(for amount: Int?) -> String? {
        guard let amount, amount > 0, let purchaseLimit else {
            return nil
        }

        let inputAmount = Decimal(amount)

        if inputAmount < purchaseLimit.minimumAmount {
            let limitText = formatLimit(purchaseLimit.minimumAmount, currencyCode: purchaseLimit.currencyCode)
            return String(localized: .fiatOnrampAmountMinimum(limitText))
        }

        if inputAmount > purchaseLimit.maximumAmount {
            let limitText = formatLimit(purchaseLimit.maximumAmount, currencyCode: purchaseLimit.currencyCode)
            return String(localized: .fiatOnrampAmountMaximum(limitText))
        }

        return nil
    }

    func formatLimit(_ amount: Decimal, currencyCode: String) -> String {
        let number = NSDecimalNumber(decimal: amount)
        limitFormatter.currencyCode = currencyCode.isEmpty ? nil : currencyCode
        if let formatted = limitFormatter.string(from: number) {
            return formatted
        }
        return currencyCode.isEmpty ? number.stringValue : "\(currencyCode) \(number)"
    }
}
