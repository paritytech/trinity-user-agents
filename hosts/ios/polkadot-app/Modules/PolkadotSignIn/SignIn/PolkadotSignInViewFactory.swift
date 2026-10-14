import Foundation

@MainActor
enum PolkadotSignInViewFactory {
    static func createView(
        serviceCoordinator: ServiceCoordinatorProtocol,
        url: URL,
        onResult: ((PolkadotSignInResult) -> Void)? = nil
    ) -> PolkadotSignInViewProtocol? {
        guard let tld = try? DotNsTldProviderFacade.shared.currentTldOrError() else {
            return nil
        }

        let interactor = PolkadotSignInInteractor(
            serviceCoordinator: serviceCoordinator,
            deviceMessageBroadcaster: MultideviceComponentFactory.makeDeviceMessageBroadcaster(
                messageExchangeModeProvider: ChatMessageExchangeModeProvider(tld: tld)
            ),
            localNetworkPermissionService: LocalNetworkPermissionService.shared,
            url: url
        )
        let wireframe = PolkadotSignInWireframe(onResult: onResult)

        let presenter = PolkadotSignInPresenter(
            interactor: interactor,
            wireframe: wireframe
        )
        let view = PolkadotSignInViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        BottomSheetViewFacade.setupBottomSheet(from: view.controller, preferredHeight: nil)

        return view
    }
}
