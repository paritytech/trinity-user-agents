import Foundation

/// What the quote the session is on charges, line by line.
final class FundingFeesPresenter: FundingScreenPresenter {
    private let wireframe: FundingFeesWireframeProtocol

    init(context: FundingFlowContext, interactor: FundingFeesInteractor, wireframe: FundingFeesWireframeProtocol) {
        self.wireframe = wireframe
        super.init(context: context, interactor: interactor)
    }
}

extension FundingFeesPresenter: FundingFeesPresenterProtocol {
    func back() {
        wireframe.back()
    }
}
