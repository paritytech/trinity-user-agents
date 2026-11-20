import UIKit

@MainActor
enum FundingDepositViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingDepositInteractor(service: wireframe.service, entryId: wireframe.context.entryId)
        let presenter = FundingDepositPresenter(
            context: wireframe.context,
            interactor: interactor,
            wireframe: wireframe
        )
        interactor.entryOutput = presenter

        let model = FundingScreenModel(state: presenter.state)
        let view = FundingScreenController(
            screen: .deposit,
            model: model,
            content: FundingDepositView(model: model, presenter: presenter).fundingScreen(),
            onLoad: presenter.setup
        )
        presenter.view = view
        return view
    }
}
