import UIKit

@MainActor
enum FundingProgressViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingProgressInteractor(service: wireframe.service, entryId: wireframe.flowState.entryId)
        let presenter = FundingProgressPresenter(
            flowState: wireframe.flowState,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: wireframe.viewModelFactory
        )
        interactor.entryOutput = presenter
        interactor.presenter = presenter

        let view = FundingProgressViewController(presenter: presenter, branding: wireframe.flowState.branding)
        presenter.view = view
        return view
    }
}
