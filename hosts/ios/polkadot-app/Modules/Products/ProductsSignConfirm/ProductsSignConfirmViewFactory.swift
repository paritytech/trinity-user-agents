import Foundation

@MainActor
enum ProductsSignConfirmViewFactory {
    static func createView(
        context: any ProductsSignConfirmContextProtocol
    ) -> PolkadotSigningViewProtocol {
        let wireframe = ProductsSignConfirmWireframe(context: context)
        let interactor = ProductsSignConfirmInteractor(context: context)
        let iconFactory: ProductIconViewModelMaking? = RootDependencyLocator.getDependency()
        let presenter = ProductsSignConfirmPresenter(
            interactor: interactor,
            wireframe: wireframe,
            requesterIcon: context.requester.productId.flatMap { iconFactory?.createViewModel(for: $0) }
        )
        let view = PolkadotSigningViewController(presenter: presenter)

        interactor.presenter = presenter
        presenter.view = view

        BottomSheetViewFacade.setupBottomSheet(from: view.controller, preferredHeight: nil)

        return view
    }
}
