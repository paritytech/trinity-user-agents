import UIKit
import UIKitExt

@MainActor
protocol ScanAddressPresentable {
    func showAddressScan(
        from view: ControllerBackedProtocol?,
        delegate: AddressScanDelegate
    )
    func hideAddressScan(from view: ControllerBackedProtocol?)
}

extension ScanAddressPresentable {
    func showAddressScan(
        from view: ControllerBackedProtocol?,
        delegate: AddressScanDelegate
    ) {
        guard
            let scanView = AddressScanViewFactory.createView(
                for: delegate,
                context: nil
            ) else {
            return
        }

        let navigationController = AppNavigationController(rootViewController: scanView.controller)
        navigationController.barSettings = .init(
            style: NavigationBarStyle(
                backgroundColor: nil,
                shadow: nil,
                shadowColor: nil,
                tintColor: .fgPrimary,
                backImage: nil,
                backgroundEffect: nil,
                titleAttributes: nil,
                largeTitleAttributes: nil
            ),
            shouldSetCloseButton: true
        )

        navigationController.modalPresentationStyle = .fullScreen

        view?.controller.present(navigationController, animated: true, completion: nil)
    }

    func hideAddressScan(from view: ControllerBackedProtocol?) {
        view?.controller.dismiss(animated: true)
    }
}
