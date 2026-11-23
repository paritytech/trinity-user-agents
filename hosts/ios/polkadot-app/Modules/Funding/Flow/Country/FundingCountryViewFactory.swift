import UIKit

@MainActor
enum FundingCountryViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingCountryInteractor(service: wireframe.service, entryId: wireframe.context.entryId)
        let presenter = FundingCountryPresenter(
            context: wireframe.context,
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
