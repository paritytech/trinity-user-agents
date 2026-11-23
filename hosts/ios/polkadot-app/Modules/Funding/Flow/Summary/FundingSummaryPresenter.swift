import Foundation
import TrUAPIHost

/// What the chosen provider will charge and pay, before the session is
/// handed to it. A provider is only chosen on its quote.
final class FundingSummaryPresenter: FundingScreenPresenter {
    weak var view: FundingSummaryViewProtocol?
    private let interactor: FundingSummaryInteractorInputProtocol
    private let wireframe: FundingSummaryWireframeProtocol
    private let viewModelFactory: FundingViewModelFactoryProtocol
    private let logger: LoggerProtocol

    init(
        context: FundingFlowContext,
        interactor: FundingSummaryInteractor,
        wireframe: FundingSummaryWireframeProtocol,
        viewModelFactory: FundingViewModelFactoryProtocol,
        logger: LoggerProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        self.logger = logger
        super.init(context: context, interactor: interactor)
    }

    override func render() {
        view?.didReceive(viewModel: viewModelFactory.makeSummaryViewModel(for: state))
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
                // A crypto top-up's address opens from the progress screen
                // once the provider reports one; a provider that serves the
                // network with its own screen reports none.
                wireframe.showProgress()
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
