import UIKit
import UIKitExt

@MainActor
protocol ScanURLPresentable {
    func showURLScan(
        from view: ControllerBackedProtocol?,
        delegate: URLScanDelegate,
        initialMessage: String?
    )
}

extension ScanURLPresentable {
    func showURLScan(
        from view: ControllerBackedProtocol?,
        delegate: URLScanDelegate,
        initialMessage: String? = nil
    ) {
        guard let scanView = URLScanViewFactory.createView(
            for: delegate,
            initialMessage: initialMessage
        ) else {
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

        navigationController.modalPresentationStyle = .fullScreen

        view?.controller.present(navigationController, animated: true)
    }
}
