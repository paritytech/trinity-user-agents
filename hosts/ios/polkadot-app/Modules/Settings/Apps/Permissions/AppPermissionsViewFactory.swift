import Foundation
import Products

@MainActor
enum AppPermissionsViewFactory {
    static func createView(
        productId: ProductId,
        productName: String
    ) -> AppPermissionsViewProtocol? {
        let coreAuthorizations: TrUAPIPermissionAuthorizing? = RootDependencyLocator.getDependency()
        let interactor = AppPermissionsInteractor(
            productId: productId,
            providerFactory: ProductPermissionDataProviderFactory(),
            repository: ProductPermissionRepository(),
            coreAuthorizations: coreAuthorizations
        )

        let wireframe = AppPermissionsWireframe()

        let presenter = AppPermissionsPresenter(
            productName: productName,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: AppPermissionsViewModelFactory()
        )

        let view = AppPermissionsViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
