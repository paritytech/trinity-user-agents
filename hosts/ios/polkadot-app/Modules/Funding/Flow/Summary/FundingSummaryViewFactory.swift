import UIKit

@MainActor
enum FundingSummaryViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingSummaryInteractor(service: wireframe.service, entryId: wireframe.flowState.entryId)
        let presenter = FundingSummaryPresenter(
            flowState: wireframe.flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: wireframe.viewModelFactory,
            logger: Logger.shared
        )
        interactor.entryOutput = presenter

        let view = FundingSummaryViewController(presenter: presenter)
        presenter.view = view
        return view
    }
}
