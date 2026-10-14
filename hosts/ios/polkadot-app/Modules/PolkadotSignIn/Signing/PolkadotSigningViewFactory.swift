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
        let presenter = PolkadotSigningPresenter(
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
