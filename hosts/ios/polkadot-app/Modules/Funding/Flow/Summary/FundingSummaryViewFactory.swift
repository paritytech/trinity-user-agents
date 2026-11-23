import UIKit

@MainActor
enum FundingSummaryViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingSummaryInteractor(service: wireframe.service, entryId: wireframe.context.entryId)
        let presenter = FundingSummaryPresenter(
            context: wireframe.context,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: wireframe.viewModelFactory,
            logger: Logger.shared
        )
        interactor.entryOutput = presenter

        let model = FundingScreenModel(state: presenter.state, viewModel: presenter.viewModel)
        let view = FundingScreenController(
            screen: .summary,
            model: model,
            content: FundingSummaryView(model: model, presenter: presenter).fundingScreen(),
            onLoad: presenter.setup
        )
        presenter.view = view
        return view
    }
}
