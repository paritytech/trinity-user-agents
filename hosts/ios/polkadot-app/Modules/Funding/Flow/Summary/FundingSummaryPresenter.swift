import Foundation
import TrUAPIHost

/// What the chosen provider will charge and pay, before the session is
/// handed to it. A provider is only chosen on its quote.
final class FundingSummaryPresenter: FundingScreenPresenter {
    private let interactor: FundingSummaryInteractorInputProtocol
    private let wireframe: FundingSummaryWireframeProtocol
    private let logger: LoggerProtocol

    init(
        context: FundingFlowContext,
        interactor: FundingSummaryInteractor,
        wireframe: FundingSummaryWireframeProtocol,
        logger: LoggerProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.logger = logger
        super.init(context: context, interactor: interactor)
    }
}

extension FundingSummaryPresenter: FundingSummaryPresenterProtocol {
    /// Hands the session to the provider whose quote is on screen. The user
    /// can leave while it is being selected, when the core drops the session
    /// and nothing more is shown.
    func start() {
        let current = state
        guard !current.isStarting, !current.hasStarted, let providerId = current.selectedProviderId,
              current.selectedQuote != nil
        else { return }

        activity.isStarting = true
        activity.startError = nil
        Task {
            defer { activity.isStarting = false }
            do {
                try await interactor.start(provider: providerId)
                if current.direction == .in, current.rail == .crypto {
                    wireframe.showDeposit()
                } else {
                    wireframe.showProgress()
                }
            } catch FundingStartError.left {
                return
            } catch let FundingStartError.notSelected(failure) {
                activity.startError = failure?.failedText ?? String(localized: .Funding.errorStartFailed)
            } catch {
                logger.error("[funding] \(context.entryId): selecting \(providerId) failed: \(error)")
                activity.startError = String(localized: .Funding.errorStartFailed)
            }
        }
    }

    func showFees() {
        wireframe.show(.fees)
    }

    func showCountry() {
        wireframe.show(.country)
    }

    func showProviders() {
        wireframe.show(.providers)
    }

    func back() {
        wireframe.back()
    }
}
