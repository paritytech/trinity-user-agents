import UIKit

@MainActor
enum FundingCancelConfirmViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingCancelConfirmInteractor(service: wireframe.service, entryId: wireframe.context.entryId)
        let presenter = FundingCancelConfirmPresenter(
            context: wireframe.context,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: wireframe.viewModelFactory
        )
        interactor.entryOutput = presenter

        let view = FundingCancelConfirmViewController(presenter: presenter)
        presenter.view = view
        return view
    }
}
