import UIKit

@MainActor
enum FundingProvidersViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingProvidersInteractor(service: wireframe.service, entryId: wireframe.flowState.entryId)
        let presenter = FundingProvidersPresenter(
            flowState: wireframe.flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: wireframe.viewModelFactory
        )
        interactor.entryOutput = presenter

        let view = FundingProvidersViewController(presenter: presenter)
        presenter.view = view
        return view
    }
}
