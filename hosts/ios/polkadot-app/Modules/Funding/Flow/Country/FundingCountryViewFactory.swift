import UIKit

@MainActor
enum FundingCountryViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingCountryInteractor(service: wireframe.service, entryId: wireframe.flowState.entryId)
        let presenter = FundingCountryPresenter(
            flowState: wireframe.flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: wireframe.viewModelFactory
        )
        interactor.entryOutput = presenter

        let view = FundingCountryViewController(presenter: presenter)
        presenter.view = view
        return view
    }
}
