import UIKit

@MainActor
enum FundingCountryViewFactory {
    static func createView(wireframe: FundingFlowWireframe) -> UIViewController {
        let interactor = FundingCountryInteractor(service: wireframe.service, entryId: wireframe.context.entryId)
        let presenter = FundingCountryPresenter(
            context: wireframe.context,
            interactor: interactor,
            wireframe: wireframe
        )
        interactor.entryOutput = presenter

        let model = FundingScreenModel(state: presenter.state)
        let view = FundingScreenController(
            screen: .country,
            model: model,
            content: FundingCountryView(model: model, presenter: presenter).fundingScreen(),
            onLoad: presenter.setup
        )
        presenter.view = view
        return view
    }
}
