import UIKit

@MainActor
enum FundingProgressViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingProgressInteractor(service: wireframe.service, entryId: wireframe.context.entryId)
        let presenter = FundingProgressPresenter(
            context: wireframe.context,
            interactor: interactor,
            wireframe: wireframe
        )
        interactor.entryOutput = presenter

        let model = FundingScreenModel(state: presenter.state)
        let view = FundingScreenController(
            screen: .progress,
            model: model,
            content: FundingProgressView(model: model, presenter: presenter).fundingScreen(),
            onLoad: presenter.setup
        )
        presenter.view = view
        return view
    }
}
