import UIKit

@MainActor
enum FundingTokenViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingTokenInteractor(service: wireframe.service, entryId: wireframe.context.entryId)
        let presenter = FundingTokenPresenter(
            context: wireframe.context,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: wireframe.viewModelFactory
        )
        interactor.entryOutput = presenter

        let view = FundingTokenViewController(presenter: presenter)
        presenter.view = view
        return view
    }
}
