import UIKit

@MainActor
enum FundingCancelConfirmViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingCancelConfirmInteractor(
            service: wireframe.service,
            entryId: wireframe.flowState.entryId
        )
        let presenter = FundingCancelConfirmPresenter(
            flowState: wireframe.flowState,
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
