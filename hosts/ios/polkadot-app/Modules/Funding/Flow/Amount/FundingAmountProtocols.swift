import Foundation
import SubstrateSdk
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
    func quote(amount: Balance?)
    func amountIssue(_ amount: Balance, spendable: Balance?) -> FundingAmountIssue?
    /// Calls `onChange` with what a payment can spend, now and on every
    /// balance change, while the screen is up.
    func followSpendable(_ onChange: @escaping @MainActor (Balance) -> Void)
    /// Calls `onChange` with every session the core holds, now and on every
    /// change, while the screen is up.
    func followEntries(_ onChange: @escaping @MainActor ([FundingEntry]) -> Void)
}

@MainActor
protocol FundingAmountWireframeProtocol: AnyObject {
    func show(_ screen: FundingFlowScreen)
    func showHistory()
    func close(then completion: (() -> Void)?)
}

extension FundingFlowWireframe: FundingAmountWireframeProtocol {}
