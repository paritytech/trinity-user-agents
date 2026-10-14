import Foundation

@MainActor
enum AppsListViewFactory {
    static func createView(
        flowStateProvider: any SPAFlowStateProviding
    ) -> AppsListViewProtocol? {
        let flowState = flowStateProvider.flowState()

        let interactor = AppsListInteractor(
            providerFactory: ProductPermissionDataProviderFactory(),
            productResolver: flowState.productResolver
        )

        let wireframe = AppsListWireframe(flowStateProvider: flowStateProvider)

        let presenter = AppsListPresenter(
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: AppsListViewModelFactory(
                iconViewModelFactory: flowState.iconViewModelFactory
            )
        )

        let view = AppsListViewController(presenter: presenter)
        view.hidesBottomBarWhenPushed = true

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
