import Foundation

@MainActor
enum RemoveDeviceViewFactory {
    static func createView(
        device: Chat.LocalDevice,
        serviceCoordinator: ServiceCoordinatorProtocol,
        onResult: @escaping (Bool) -> Void
    ) -> RemoveDeviceViewProtocol? {
        guard let tld = try? DotNsTldProviderFacade.shared.currentTldOrError() else {
            return nil
        }

        let wireframe = RemoveDeviceWireframe()

        let interactor = RemoveDeviceInteractor(
            localDeviceRepository: LocalDeviceRepositoryFactory().createRepository(forFilter: nil),
            serviceCoordinator: serviceCoordinator,
            deviceMessageBroadcaster: MultideviceComponentFactory.makeDeviceMessageBroadcaster(
                messageExchangeModeProvider: ChatMessageExchangeModeProvider(tld: tld)
            )
        )

        let presenter = RemoveDevicePresenter(
            interactor: interactor,
            wireframe: wireframe,
            device: device,
            onResult: onResult
        )

        let view = RemoveDeviceViewController(presenter: presenter)
        interactor.presenter = presenter
        presenter.view = view

        BottomSheetViewFacade.setupBottomSheet(from: view, preferredHeight: nil)

        return view
    }
}
