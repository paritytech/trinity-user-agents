import Foundation
import TrUAPIHost

/// Where a started session stands, and how it ended once it did. An ended
/// session on screen has told the user how it went.
final class FundingProgressPresenter: FundingScreenPresenter {
    weak var view: FundingProgressViewProtocol?
    private let interactor: FundingProgressInteractorInputProtocol
    private let wireframe: FundingProgressWireframeProtocol
    private let viewModelFactory: FundingViewModelFactoryProtocol
    private let logger: LoggerProtocol
    private var hasAcknowledged = false
    private var hasShownDeposit = false

    init(
        flowState: FundingFlowState,
        interactor: FundingProgressInteractor,
        wireframe: FundingProgressWireframeProtocol,
        viewModelFactory: FundingViewModelFactoryProtocol,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        self.logger = logger
        super.init(flowState: flowState, interactor: interactor)
    }

    override func render() {
        view?.didReceive(viewModel: viewModelFactory.makeProgressViewModel(for: model))
    }

    override func setup() {
        super.setup()
        showDepositOnceReported()
        refreshWhenTakingLonger()
    }

    override func entryDidChange(from _: FundingEntry) {
        showDepositOnceReported()
        refreshWhenTakingLonger()
        guard entry.hasEnded, !hasAcknowledged else { return }

        hasAcknowledged = true
        interactor.acknowledge()
    }
}

private extension FundingProgressPresenter {
    /// Opens where to send a crypto top-up, once, as soon as its provider
    /// reports the address: on the app's own network that is the whole flow,
    /// while a provider that serves another network with its own screen
    /// reports no address, so only its screen opens.
    func showDepositOnceReported() {
        let current = model
        guard !hasShownDeposit, current.direction == .in, !current.hasReached(.received),
              case .crypto? = current.progress?.deposit
        else { return }

        hasShownDeposit = true
        wireframe.show(.deposit)
    }

    func refreshWhenTakingLonger() {
        let current = model
        interactor.refreshWhenTakingLonger(atMs: current.hasStarted ? current.progress?.longerAtMs : nil)
    }
}

extension FundingProgressPresenter: FundingProgressInteractorOutputProtocol {
    func didReceiveCancelOfferAnswer(result: Result<CancelConfirmation, Error>) {
        defer { activity.isAnsweringOffer = false }
        switch result {
        case let .success(answer):
            answer.toast.map(wireframe.showToast)
        case let .failure(error):
            logger.error("[funding] \(flowState.entryId): answering the cancel offer failed: \(error)")
            wireframe.showToast(String(localized: .Funding.errorCancelFailed))
        }
    }
}

extension FundingProgressPresenter: FundingProgressPresenterProtocol {
    func showFees() {
        wireframe.show(.fees)
    }

    func showDeposit() {
        wireframe.show(.deposit)
    }

    func confirmCancel() {
        wireframe.show(.cancelConfirm)
    }

    /// Answers the provider's cancel offer: `terms` are the ones the user
    /// accepted, as shown, or nil to keep going. The core accepts only terms
    /// that still match the offer.
    func answerCancelOffer(accepting terms: CancelTerms?) {
        guard !activity.isAnsweringOffer else { return }

        activity.isAnsweringOffer = true
        interactor.answerCancelOffer(accepting: terms)
    }

    func didCopy() {
        wireframe.showCopied()
    }

    func startOver() {
        wireframe.startOver()
    }

    func close() {
        wireframe.close(then: nil)
    }
}
