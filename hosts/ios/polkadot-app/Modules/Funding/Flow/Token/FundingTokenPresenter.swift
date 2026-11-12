import Foundation

/// Which token a crypto session pays in. Crypto value in starts on the core's
/// best quote from its deposit screen; value out goes to the summary.
final class FundingTokenPresenter: FundingScreenPresenter {
    private let interactor: FundingTokenInteractorInputProtocol
    private let wireframe: FundingTokenWireframeProtocol
    private let logger: LoggerProtocol

    init(
        context: FundingFlowContext,
        interactor: FundingTokenInteractor,
        wireframe: FundingTokenWireframeProtocol,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.logger = logger
        super.init(context: context, interactor: interactor)
    }
}

extension FundingTokenPresenter: FundingTokenPresenterProtocol {
    func chooseToken(_ symbol: String) {
        context.update { $0.asset = symbol }
        requestQuote()
        guard context.direction == .in else {
            wireframe.show(.summary)
            return
        }

        Task { [interactor, logger, entryId = context.entryId] in
            do {
                try await interactor.startWhenQuoted()
            } catch {
                logger.error("[funding] \(entryId): starting on the best quote failed: \(error)")
            }
        }
        wireframe.show(.deposit)
    }

    func back() {
        wireframe.back()
    }
}
