import Foundation
import Products

@MainActor
enum AppPermissionsViewFactory {
    static func createView(
        productId: ProductId,
        productName: String
    ) -> AppPermissionsViewProtocol? {
        let permissions: AppPermissionSettings
        do {
            permissions = try AppPermissionSettings()
        } catch {
            Logger.shared.error("Permission settings unavailable: \(error)")
            return nil
        }

        let interactor = AppPermissionsInteractor(
            productId: productId,
            permissions: permissions
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
