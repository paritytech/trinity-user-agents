import UIKit

@MainActor
enum FundingNetworkViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingNetworkInteractor(service: wireframe.service, entryId: wireframe.context.entryId)
        let presenter = FundingNetworkPresenter(
            context: wireframe.context,
            interactor: interactor,
            wireframe: wireframe
        )
        interactor.entryOutput = presenter

        let model = FundingScreenModel(state: presenter.state)
        let view = FundingScreenController(
            screen: .network,
            model: model,
            content: FundingNetworkView(model: model, presenter: presenter).fundingScreen(),
            onLoad: presenter.setup
        )
        presenter.view = view
        return view
    }
}
