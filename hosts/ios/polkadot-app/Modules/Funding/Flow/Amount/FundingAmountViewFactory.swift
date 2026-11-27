import UIKit

@MainActor
enum FundingAmountViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingAmountInteractor(
            service: wireframe.service,
            entryId: wireframe.flowState.entryId,
            environment: wireframe.environment
        )
        let presenter = FundingAmountPresenter(
            flowState: wireframe.flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: wireframe.viewModelFactory
        )
        interactor.entryOutput = presenter

        let view = FundingAmountViewController(presenter: presenter)
        presenter.view = view
        return view
    }
}
