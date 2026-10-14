import UIKit

@MainActor
final class IdentityDetailsWireframe: IdentityDetailsWireframeProtocol {
    func presentQrSheet(from view: IdentityDetailsViewProtocol?) {
        guard let view else { return }

        let sheet = IdentityQrSheetViewFactory.createView(viewModel: view.viewModel)
        view.controller.present(sheet, animated: true)
    }
}
