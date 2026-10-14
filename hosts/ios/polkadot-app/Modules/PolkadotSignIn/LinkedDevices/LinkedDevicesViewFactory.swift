import Foundation

@MainActor
enum LinkedDevicesViewFactory {
    static func createView(
        serviceCoordinator: ServiceCoordinatorProtocol
    ) -> LinkedDevicesViewProtocol? {
        let wireframe = LinkedDevicesWireframe(
            serviceCoordinator: serviceCoordinator
        )
        let interactor = LinkedDevicesInteractor()
        let presenter = LinkedDevicesPresenter(
            interactor: interactor,
            wireframe: wireframe
        )
        let view = LinkedDevicesViewController(presenter: presenter)
        view.hidesBottomBarWhenPushed = true

        interactor.presenter = presenter
        presenter.view = view

        return view
    }
}
