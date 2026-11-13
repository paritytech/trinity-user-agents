import Foundation

/// Which token a crypto session pays in or receives; the summary shows the
/// provider, fees and total before the user starts.
final class FundingTokenPresenter: FundingScreenPresenter {
    private let wireframe: FundingTokenWireframeProtocol

    init(
        context: FundingFlowContext,
        interactor: FundingTokenInteractor,
        wireframe: FundingTokenWireframeProtocol
    ) {
        self.wireframe = wireframe
        super.init(context: context, interactor: interactor)
    }
}

extension FundingTokenPresenter: FundingTokenPresenterProtocol {
    func chooseToken(_ symbol: String) {
        context.update { $0.asset = symbol }
        requestQuote()
        wireframe.show(.summary)
    }

    func back() {
        wireframe.back()
    }
}
