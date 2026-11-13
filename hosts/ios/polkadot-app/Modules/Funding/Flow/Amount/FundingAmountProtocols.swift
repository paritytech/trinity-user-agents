import Foundation
import TrUAPIHost

typealias FundingAmountViewProtocol = FundingScreenViewProtocol

@MainActor
protocol FundingAmountPresenterProtocol: AnyObject {
    func setup()
    func selectRail(_ rail: FundingRail)
    func amountTextChanged(_ text: String)
    func continueFromAmount()
    func showHistory()
    func close()
}

@MainActor
protocol FundingAmountInteractorInputProtocol: AnyObject {
    func setup()
    func select(_ selection: FundingSelection)
    func quote(amount: U128?)
    func amountIssue(_ amount: U128, spendable: U128?) -> FundingAmountIssue?
    /// Calls `onChange` with what a payment can spend, now and on every
    /// balance change, while the screen is up.
    func followSpendable(_ onChange: @escaping @MainActor (Decimal) -> Void)
}

@MainActor
protocol FundingAmountWireframeProtocol: AnyObject {
    func show(_ screen: FundingFlowScreen)
    func showHistory()
    func close(then completion: (() -> Void)?)
}

extension FundingFlowWireframe: FundingAmountWireframeProtocol {}
