import UIKit
import UIKitExt

@MainActor
protocol WalletQRScanPresentable {
    func showWalletQRScan(from view: ControllerBackedProtocol?, delegate: WalletQRScanDelegate)
}

@MainActor
extension WalletQRScanPresentable {
    func showWalletQRScan(from view: ControllerBackedProtocol?, delegate: WalletQRScanDelegate) {
        guard let scanView = WalletQRScanViewFactory.createView(for: delegate) else {
            return
        }

        let navigationController = AppNavigationController(rootViewController: scanView.controller)

        navigationController.barSettings = .init(
            style: NavigationBarStyle(
                backgroundColor: nil,
                shadow: nil,
                shadowColor: nil,
                tintColor: .white100,
                backImage: nil,
                backgroundEffect: nil,
                titleAttributes: nil,
                largeTitleAttributes: nil
            ),
            shouldSetCloseButton: true
        )

        view?.controller.present(navigationController, animated: true)
    }

    func showWalletQRScan(from view: ControllerBackedProtocol?, resultHandler: WalletQRScanResultHandler) {
        resultHandler.view = view
        showWalletQRScan(from: view, delegate: resultHandler)
    }
}
