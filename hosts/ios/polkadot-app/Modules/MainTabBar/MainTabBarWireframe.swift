import UIKit
import PolkadotUI
import Products

@MainActor
final class MainTabBarWireframe: MainTabBarWireframeProtocol {
    private let serviceCoordinator: ServiceCoordinatorProtocol
    private let scanResultHandler: WalletQRScanDelegate
    private let moduleNavigator: ModuleNavigating

    init(
        serviceCoordinator: ServiceCoordinatorProtocol,
        scanResultHandler: WalletQRScanDelegate,
        moduleNavigator: ModuleNavigating
    ) {
        self.serviceCoordinator = serviceCoordinator
        self.scanResultHandler = scanResultHandler
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

    #if FEATURE_INPUT
        func openChat(_ model: ChatOpenModel) {
            moduleNavigator.openChat(model)
        }
    #else
        func showSearchContact(from view: MainTabBarViewProtocol?) {
            guard let search = SearchContactViewFactory.createView(
                onChatFound: { [moduleNavigator] model in
                    moduleNavigator.openChat(model)
                }
            ) else {
                return
            }

            search.controller.modalPresentationStyle = .fullScreen
            search.controller.modalTransitionStyle = .crossDissolve
            view?.controller.present(search.controller, animated: true)
        }
    #endif
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
        view?.select(tab: .settings)

        guard
            let settingsNav = (view as? MainTabBarViewController)?.view(for: .settings) as? UINavigationController,
            let settingsRoot = settingsNav.viewControllers.first,
            let linkedDevicesView = LinkedDevicesViewFactory.createView(serviceCoordinator: serviceCoordinator),
            let detailsView = DeviceDetailsViewFactory.createView(
                device: device,
                serviceCoordinator: serviceCoordinator
            )
        else {
            return
        }

        settingsNav.setViewControllers(
            [settingsRoot, linkedDevicesView.controller, detailsView.controller],
            animated: true
        )
    }
}
