import Foundation
import Products

@MainActor
enum AppDetailViewFactory {
    static func createView(
        productId: ProductId,
        flowStateProvider: any SPAFlowStateProviding
    ) -> AppDetailViewProtocol? {
        let flowState = flowStateProvider.flowState()
        let wireframe = AppDetailWireframe()

        let interactor = AppDetailInteractor(
            productId: productId,
            productResolver: flowState.productResolver
        )

        let presenter = AppDetailPresenter(
            productId: productId,
            interactor: interactor,
            wireframe: wireframe,
            iconViewModelFactory: flowState.iconViewModelFactory
        )

        let view = AppDetailViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
