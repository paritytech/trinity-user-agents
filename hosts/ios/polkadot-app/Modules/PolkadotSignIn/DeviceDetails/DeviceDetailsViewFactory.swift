import Foundation

@MainActor
enum DeviceDetailsViewFactory {
    static func createView(
        device: Chat.LocalDevice,
        serviceCoordinator: ServiceCoordinatorProtocol
    ) -> DeviceDetailsViewProtocol? {
        let wireframe = DeviceDetailsWireframe(serviceCoordinator: serviceCoordinator)

        let presenter = DeviceDetailsPresenter(
            wireframe: wireframe,
            viewModelFactory: DeviceDetailsViewModelFactory(),
            device: device
        )

        let view = DeviceDetailsViewController(presenter: presenter)

        presenter.view = view

        return view
    }
}
