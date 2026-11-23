import UIKit

@MainActor
enum FundingFeesViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingFeesInteractor(service: wireframe.service, entryId: wireframe.context.entryId)
        let presenter = FundingFeesPresenter(
            context: wireframe.context,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: wireframe.viewModelFactory
        )
        interactor.entryOutput = presenter

        let model = FundingScreenModel(state: presenter.state, viewModel: presenter.viewModel)
        let view = FundingScreenController(
            screen: .fees,
            model: model,
            content: FundingFeesView(model: model, presenter: presenter).fundingScreen(),
            onLoad: presenter.setup
        )
        presenter.view = view
        return view
    }
}
