import Foundation

@MainActor
enum PolkadotSigningViewFactory {
    static func createView(
        signingContext: PolkadotSigningContextProtocol
    ) -> PolkadotSigningViewProtocol? {
        let wireframe = PolkadotSigningWireframe(signingContext: signingContext)
        let interactor = PolkadotSigningInteractor(
            signingContext: signingContext
        )
        let iconFactory: ProductIconViewModelMaking? = RootDependencyLocator.getDependency()
        let presenter = PolkadotSigningPresenter(
            interactor: interactor,
            wireframe: wireframe,
            requesterIcon: signingContext.requester.productId.flatMap { iconFactory?.createViewModel(for: $0) }
        )
        let view = PolkadotSigningViewController(presenter: presenter)

        interactor.presenter = presenter
        presenter.view = view

        BottomSheetViewFacade.setupBottomSheet(from: view.controller, preferredHeight: nil)

        return view
    }
}
