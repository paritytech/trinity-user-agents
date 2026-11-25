import UIKit

@MainActor
enum FundingDepositViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingDepositInteractor(service: wireframe.service, entryId: wireframe.flowState.entryId)
        let presenter = FundingDepositPresenter(
            flowState: wireframe.flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: wireframe.viewModelFactory
        )
        interactor.entryOutput = presenter

        let view = FundingDepositViewController(presenter: presenter)
        presenter.view = view
        return view
    }
}
