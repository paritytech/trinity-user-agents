import UIKit
import PolkadotUI
import UIKitExt

@MainActor
enum ShareViewFactory {
    static func createView(
        items: [ShareItem],
        host: ControllerBackedProtocol
    ) -> ShareViewProtocol {
        let composer = ShareContentComposer()
        let wireframe = ShareWireframe(host: host, composer: composer)

        let interactor = ShareInteractor(
            chatContactDataProviderFactory: ChatContactDataProviderFactory(),
            composer: composer,
            productRooms: RootDependencyLocator.getDependency() as ShareProductRooms?
        )

        let presenter = SharePresenter(
            items: items,
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: ShareViewModelFactory()
        )

        let view = ShareViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        BottomSheetViewFacade.setupBottomSheet(from: view)

        return view
    }
}
