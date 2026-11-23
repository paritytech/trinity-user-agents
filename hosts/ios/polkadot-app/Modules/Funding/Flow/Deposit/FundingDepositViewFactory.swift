import UIKit

@MainActor
enum FundingDepositViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingDepositInteractor(service: wireframe.service, entryId: wireframe.context.entryId)
        let presenter = FundingDepositPresenter(
            context: wireframe.context,
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
