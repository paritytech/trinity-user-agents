import Foundation

/// Which network a crypto session pays on, from those the core says a route
/// takes.
final class FundingNetworkPresenter: FundingScreenPresenter {
    private let wireframe: FundingNetworkWireframeProtocol

    init(
        context: FundingFlowContext,
        interactor: FundingNetworkInteractor,
        wireframe: FundingNetworkWireframeProtocol
    ) {
        self.wireframe = wireframe
        super.init(context: context, interactor: interactor)
    }
}

extension FundingNetworkPresenter: FundingNetworkPresenterProtocol {
    func chooseNetwork(_ id: String) {
        context.update { draft in
            draft.network = id
            draft.asset = nil
        }
        reportSelection()
        wireframe.show(.token)
    }

    func back() {
        wireframe.back()
    }
}
