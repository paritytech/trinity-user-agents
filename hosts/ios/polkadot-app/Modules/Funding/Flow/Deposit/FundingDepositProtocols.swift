import Foundation

typealias FundingDepositViewProtocol = FundingScreenViewProtocol

@MainActor
protocol FundingDepositPresenterProtocol: AnyObject {
    func setup()
    func back()
    func showProgress()
    func confirmCancel()
    func close()
}

@MainActor
protocol FundingDepositInteractorInputProtocol: AnyObject {
    func setup()
    /// The user left before the session started: the core no longer starts
    /// it on its best quote.
    func stopStarting()
}

@MainActor
protocol FundingDepositWireframeProtocol: AnyObject {
    func show(_ screen: FundingFlowScreen)
    func showProgress()
    func back()
    func close(then completion: (() -> Void)?)
}

extension FundingFlowWireframe: FundingDepositWireframeProtocol {}
