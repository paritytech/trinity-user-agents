import UIKit
import PolkadotUI
import UIKit_iOS

@MainActor
enum IdentityQrSheetViewFactory {
    static func createView(viewModel: IdentityDetailsViewModel) -> UIViewController {
        let wireframe = IdentityQrSheetWireframe()
        let presenter = IdentityQrSheetPresenter(wireframe: wireframe)
        let view = IdentityQrSheetViewController(presenter: presenter, viewModel: viewModel)
        presenter.view = view

        BottomSheetViewFacade.setupBottomSheet(from: view)

        return view
    }
}
