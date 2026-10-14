import UIKit
import PolkadotUI

@MainActor
final class LinkedDevicesWireframe: LinkedDevicesWireframeProtocol {
    private let serviceCoordinator: ServiceCoordinatorProtocol
    private let schemeNormalizer: DeeplinkSchemeNormalizing

    init(
        serviceCoordinator: ServiceCoordinatorProtocol,
        schemeNormalizer: DeeplinkSchemeNormalizing = DeeplinkSchemeNormalizer()
    ) {
        self.serviceCoordinator = serviceCoordinator
        self.schemeNormalizer = schemeNormalizer
    }

    func showDeviceDetails(from view: LinkedDevicesViewProtocol?, device: Chat.LocalDevice) {
        guard let detailsView = DeviceDetailsViewFactory.createView(
            device: device,
            serviceCoordinator: serviceCoordinator
        ) else {
            return
        }

        view?.controller.navigationController?.pushViewController(
            detailsView.controller,
            animated: true
        )
    }

    func completeOpeningURL(from view: LinkedDevicesViewProtocol?, url: URL) {
        let normalizedURL = schemeNormalizer.normalize(url)

        if view?.controller.presentedViewController != nil {
            view?.controller.dismiss(animated: true) {
                UIApplication.shared.open(normalizedURL)
            }
        } else {
            UIApplication.shared.open(normalizedURL)
        }
    }
}
