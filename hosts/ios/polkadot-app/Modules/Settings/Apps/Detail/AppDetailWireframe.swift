import Foundation
import Products

@MainActor
final class AppDetailWireframe: AppDetailWireframeProtocol {
    func showPermissions(
        productId: ProductId,
        productName: String,
        from view: AppDetailViewProtocol?
    ) {
        guard let permissionsView = AppPermissionsViewFactory.createView(
            productId: productId,
            productName: productName
        ) else {
            return
        }

        view?.controller.navigationController?.pushViewController(
            permissionsView.controller,
            animated: true
        )
    }
}
