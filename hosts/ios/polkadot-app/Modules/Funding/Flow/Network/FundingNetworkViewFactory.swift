import UIKit

@MainActor
enum FundingNetworkViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingNetworkInteractor(service: wireframe.service, entryId: wireframe.flowState.entryId)
        let presenter = FundingNetworkPresenter(
            flowState: wireframe.flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: wireframe.viewModelFactory
        )
        interactor.entryOutput = presenter

        let view = FundingNetworkViewController(presenter: presenter)
        presenter.view = view
        return view
    }
}
