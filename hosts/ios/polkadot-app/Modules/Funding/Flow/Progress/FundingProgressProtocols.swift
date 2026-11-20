import Foundation
import TrUAPIHost

typealias FundingProgressViewProtocol = FundingScreenViewProtocol

@MainActor
protocol FundingProgressPresenterProtocol: AnyObject {
    func setup()
    func showFees()
    func showDeposit()
    func confirmCancel()
    func answerCancelOffer(accepting terms: CancelTerms?)
    /// Reads the session again, as when it turns to taking longer.
    func refresh()
    func startOver()
    func close()
}

@MainActor
protocol FundingProgressInteractorInputProtocol: AnyObject {
    func setup()
    func refresh()
    func answerCancelOffer(accepting terms: CancelTerms?) async throws -> CancelConfirmation
    /// The user was shown how the session ended.
    func acknowledge()
}

@MainActor
protocol FundingProgressWireframeProtocol: AnyObject {
    func show(_ screen: FundingFlowScreen)
    func close(then completion: (() -> Void)?)
    func startOver()
    func showToast(_ message: String)
}

extension FundingFlowWireframe: FundingProgressWireframeProtocol {}
