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
    /// The user copied one of the session's details.
    func didCopy()
    func startOver()
    func close()
}

@MainActor
protocol FundingProgressInteractorInputProtocol: AnyObject {
    func setup()
    /// Reads the session again at `longerAtMs`, when the core turns it to
    /// taking longer without a change to announce it; nil reads nothing.
    func refreshWhenTakingLonger(atMs longerAtMs: UInt64?)
    /// Answers the provider's cancel offer; the outcome comes back through
    /// `didReceiveCancelOfferAnswer(result:)`.
    func answerCancelOffer(accepting terms: CancelTerms?)
    /// The user was shown how the session ended.
    func acknowledge()
}

@MainActor
protocol FundingProgressInteractorOutputProtocol: FundingEntryInteractorOutputProtocol {
    func didReceiveCancelOfferAnswer(result: Result<CancelConfirmation, Error>)
}

@MainActor
protocol FundingProgressWireframeProtocol: FundingToastPresentable {
    func show(_ screen: FundingFlowScreen)
    func close(then completion: (() -> Void)?)
    func startOver()
}

extension FundingFlowWireframe: FundingProgressWireframeProtocol {}
