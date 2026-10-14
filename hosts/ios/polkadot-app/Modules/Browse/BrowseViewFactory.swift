import Foundation
import Products
import SwiftyBeaver

enum BrowseViewFactory {
    @MainActor
    static func createView(
        flowStateProvider: any SPAFlowStateProviding
    ) -> BrowseViewProtocol {
        let logger = Logger.shared
        let spaFlowState = flowStateProvider.flowState()

        let interactor = BrowseInteractor(
            hostProvider: spaFlowState.hostProvider,
            logger: logger
        )
        let wireframe = BrowseWireframe(flowStateProvider: flowStateProvider)
        let presenter = BrowsePresenter(interactor: interactor, wireframe: wireframe)

        let viewController = BrowseViewController(presenter: presenter)

        interactor.presenter = presenter
        presenter.view = viewController

        return viewController
    }
}
