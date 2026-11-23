import UIKit

@MainActor
enum FundingProvidersViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingProvidersInteractor(service: wireframe.service, entryId: wireframe.context.entryId)
        let presenter = FundingProvidersPresenter(
            context: wireframe.context,
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
