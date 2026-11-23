import Foundation

/// What the quote the session is on charges, line by line.
final class FundingFeesPresenter: FundingScreenPresenter<FundingFeesViewModel?> {
    private let wireframe: FundingFeesWireframeProtocol

    init(
        context: FundingFlowContext,
        interactor: FundingFeesInteractor,
        wireframe: FundingFeesWireframeProtocol,
        viewModelFactory: FundingViewModelFactoryProtocol
    ) {
        self.wireframe = wireframe
        super.init(context: context, interactor: interactor) { viewModelFactory.makeFeesViewModel(for: $0) }
    }
}

extension FundingFeesPresenter: FundingFeesPresenterProtocol {
    func back() {
        wireframe.back()
    }
}
