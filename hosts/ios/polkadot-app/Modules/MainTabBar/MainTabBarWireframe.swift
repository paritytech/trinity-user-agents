import UIKit
import PolkadotUI
import Products
import UIKitExt

@MainActor
final class MainTabBarWireframe: MainTabBarWireframeProtocol {
    private let serviceCoordinator: ServiceCoordinatorProtocol
    private let tabFactory: TabFactoryProtocol
    private let moduleNavigator: ModuleNavigating
    private weak var presentedSettings: UINavigationController?

    init(
        serviceCoordinator: ServiceCoordinatorProtocol,
        tabFactory: TabFactoryProtocol,
        moduleNavigator: ModuleNavigating
    ) {
        self.serviceCoordinator = serviceCoordinator
        self.tabFactory = tabFactory
        self.moduleNavigator = moduleNavigator
    }

    func showPolkadotSignIn(with url: URL, view: MainTabBarViewProtocol?) {
        guard let signInView = PolkadotSignInViewFactory.createView(
            serviceCoordinator: serviceCoordinator,
            url: url,
            onResult: { [weak self, weak view] result in
                self?.handleSignInResult(result, view: view)
            }
        ) else {
            return
        }
        view?.controller.present(signInView.controller, animated: true)
    }

    func openProduct(productId: ProductId) {
        ProductOpener(navigator: moduleNavigator).open(productId: productId)
    }

    func openChat(_ model: ChatOpenModel) {
        moduleNavigator.openChat(model)
    }

    func showConnectionStatus(_ viewModel: ConnectionStatusSheetViewModel, from view: MainTabBarViewProtocol?) {
        let sheet = ConnectionStatusSheetViewFactory.createView(viewModel: viewModel)
        view?.controller.topmostPresented.present(sheet, animated: true)
    }

    func showSettings(from view: MainTabBarViewProtocol?) {
        guard let settings = tabFactory.makeSettingsNavigation() else {
            return
        }
        presentedSettings = settings
        view?.controller.topmostPresented.present(settings, animated: true)
    }
}

private extension MainTabBarWireframe {
    func handleSignInResult(_ result: PolkadotSignInResult, view: MainTabBarViewProtocol?) {
        switch result {
        case let .success(device):
            view?.controller.showToast(
                message: String(localized: .linkedDevicesSignInDeviceConnected),
                type: .success
            )
            showDeviceDetails(device, view: view)
        case let .noFreeSlots(message):
            let controller = NoSlotsAvailableViewFactory.createView(message: message)
            view?.controller.present(controller, animated: true)
        case .failed:
            view?.controller.showToast(
                message: String(localized: .linkedDevicesSignInError),
                type: .error
            )
        }
    }

    func showDeviceDetails(_ device: Chat.LocalDevice, view: MainTabBarViewProtocol?) {
        guard
            let presenter = view?.controller.topmostPresented,
            let linkedDevicesView = LinkedDevicesViewFactory.createView(serviceCoordinator: serviceCoordinator),
            let detailsView = DeviceDetailsViewFactory.createView(
                device: device,
                serviceCoordinator: serviceCoordinator
            )
        else {
            return
        }

        if let settings = presentedSettings, settings === presenter {
            settings.pushViewController(linkedDevicesView.controller, animated: false)
            settings.pushViewController(detailsView.controller, animated: true)
            return
        }

        guard let settings = tabFactory.makeSettingsNavigation() else {
            return
        }
        presentedSettings = settings
        settings.pushViewController(linkedDevicesView.controller, animated: false)
        settings.pushViewController(detailsView.controller, animated: false)
        presenter.present(settings, animated: true)
    }
}
