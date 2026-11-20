import Foundation
import TrUAPIHost

/// Asks before cancelling. The core ends a session no money moved on, and
/// otherwise asks the provider, who answers with an offer or a refusal.
final class FundingCancelConfirmPresenter: FundingScreenPresenter {
    private let interactor: FundingCancelConfirmInteractorInputProtocol
    private let wireframe: FundingCancelConfirmWireframeProtocol
    private let logger: LoggerProtocol

    init(
        context: FundingFlowContext,
        interactor: FundingCancelConfirmInteractor,
        wireframe: FundingCancelConfirmWireframeProtocol,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.logger = logger
        super.init(context: context, interactor: interactor)
    }
}

extension FundingCancelConfirmPresenter: FundingCancelConfirmPresenterProtocol {
    func requestCancel() {
        guard !activity.isCancelling else { return }

        activity.isCancelling = true
        Task {
            defer { activity.isCancelling = false }
            do {
                var outcome = try await interactor.cancel()
                if outcome == .requested, state.isUnpaidTopUp {
                    // Nothing was paid, so the second cancel ends the session.
                    outcome = try await interactor.cancel()
                }
                switch outcome.followUp {
                case .close:
                    wireframe.close(then: nil)
                case let .toast(message):
                    leave(toast: message)
                }
            } catch {
                logger.error("[funding] \(context.entryId): cancel failed: \(error)")
                leave(toast: String(localized: .Funding.errorCancelFailed))
            }
        }
    }

    func back() {
        wireframe.back()
    }
}

private extension FundingCancelConfirmPresenter {
    func leave(toast message: String) {
        wireframe.back()
        wireframe.showToast(message)
    }
}
