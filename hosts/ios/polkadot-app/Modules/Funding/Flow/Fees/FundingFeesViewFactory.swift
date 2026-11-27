import UIKit

@MainActor
enum FundingFeesViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingFeesInteractor(service: wireframe.service, entryId: wireframe.flowState.entryId)
        let presenter = FundingFeesPresenter(
            flowState: wireframe.flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: wireframe.viewModelFactory
        )
        interactor.entryOutput = presenter

        let view = FundingFeesViewController(presenter: presenter)
        presenter.view = view
        return view
    }
}
