import UIKit

@MainActor
enum FundingAmountViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingAmountInteractor(
            service: wireframe.service,
            entryId: wireframe.context.entryId,
            environment: wireframe.environment
        )
        let presenter = FundingAmountPresenter(
            context: wireframe.context,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: wireframe.viewModelFactory
        )
        interactor.entryOutput = presenter

        let model = FundingScreenModel(state: presenter.state, viewModel: presenter.viewModel)
        let view = FundingScreenController(
            screen: .amount,
            model: model,
            content: FundingAmountView(model: model, presenter: presenter).fundingScreen(),
            onLoad: presenter.setup
        )
        presenter.view = view
        return view
    }
}
