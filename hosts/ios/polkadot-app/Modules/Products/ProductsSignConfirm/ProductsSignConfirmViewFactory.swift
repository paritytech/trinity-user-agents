import Foundation

@MainActor
enum ProductsSignConfirmViewFactory {
    static func createView(
        context: any ProductsSignConfirmContextProtocol
    ) -> PolkadotSigningViewProtocol {
        let wireframe = ProductsSignConfirmWireframe(context: context)
        let interactor = ProductsSignConfirmInteractor(context: context)
        let presenter = ProductsSignConfirmPresenter(
            interactor: interactor,
            wireframe: wireframe
        )
        let view = PolkadotSigningViewController(presenter: presenter)

        interactor.presenter = presenter
        presenter.view = view

        BottomSheetViewFacade.setupBottomSheet(from: view.controller, preferredHeight: nil)

        return view
    }
}
