import UIKit

@MainActor
enum FundingTokenViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingTokenInteractor(service: wireframe.service, entryId: wireframe.flowState.entryId)
        let presenter = FundingTokenPresenter(
            flowState: wireframe.flowState,
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
