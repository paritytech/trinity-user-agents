import UIKit

@MainActor
enum FundingTokenViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingTokenInteractor(service: wireframe.service, entryId: wireframe.context.entryId)
        let presenter = FundingTokenPresenter(context: wireframe.context, interactor: interactor, wireframe: wireframe)
        interactor.entryOutput = presenter

        let model = FundingScreenModel(state: presenter.state)
        let view = FundingScreenController(
            screen: .token,
            model: model,
            content: FundingTokenView(model: model, presenter: presenter).fundingScreen(),
            onLoad: presenter.setup
        )
        presenter.view = view
        return view
    }
}
