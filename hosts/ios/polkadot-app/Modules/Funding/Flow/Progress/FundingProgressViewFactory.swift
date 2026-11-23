import UIKit

@MainActor
enum FundingProgressViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingProgressInteractor(service: wireframe.service, entryId: wireframe.context.entryId)
        let presenter = FundingProgressPresenter(
            context: wireframe.context,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: wireframe.viewModelFactory
        )
        interactor.entryOutput = presenter

        let view = FundingProgressViewController(presenter: presenter, branding: wireframe.context.branding)
        presenter.view = view
        return view
    }
}
