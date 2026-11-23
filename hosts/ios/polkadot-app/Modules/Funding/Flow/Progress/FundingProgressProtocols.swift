import Foundation
import UIKitExt
import TrUAPIHost

@MainActor
protocol FundingProgressViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: FundingProgressViewModel)
}

@MainActor
protocol FundingProgressPresenterProtocol: AnyObject {
    func setup()
    func showFees()
    func showDeposit()
    func confirmCancel()
    func answerCancelOffer(accepting terms: CancelTerms?)
    func startOver()
    func close()
}

@MainActor
protocol FundingProgressInteractorInputProtocol: AnyObject {
    func setup()
    /// Reads the session again at `longerAtMs`, when the core turns it to
    /// taking longer without a change to announce it; nil reads nothing.
    func refreshWhenTakingLonger(atMs longerAtMs: UInt64?)
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
