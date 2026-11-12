import Foundation
import TrUAPIHost

/// Where a started session stands, and how it ended once it did. An ended
/// session on screen has told the user how it went.
final class FundingProgressPresenter: FundingScreenPresenter {
    private let interactor: FundingProgressInteractorInputProtocol
    private let wireframe: FundingProgressWireframeProtocol
    private let logger: LoggerProtocol
    private var hasAcknowledged = false

    init(
        context: FundingFlowContext,
        interactor: FundingProgressInteractor,
        wireframe: FundingProgressWireframeProtocol,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.logger = logger
        super.init(context: context, interactor: interactor)
    }

    override func entryDidChange(from _: FundingEntry) {
        guard entry.hasEnded, !hasAcknowledged else { return }

        hasAcknowledged = true
        interactor.acknowledge()
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
        Task {
            defer { activity.isAnsweringOffer = false }
            do {
                let answer = try await interactor.answerCancelOffer(accepting: terms)
                answer.toast.map(wireframe.showToast)
            } catch {
                logger.error("[funding] \(context.entryId): answering the cancel offer failed: \(error)")
                wireframe.showToast(String(localized: .Funding.errorCancelFailed))
            }
        }
    }

    func refresh() {
        interactor.refresh()
    }

    func startOver() {
        wireframe.startOver()
    }

    func close() {
        wireframe.close(then: nil)
    }
}
