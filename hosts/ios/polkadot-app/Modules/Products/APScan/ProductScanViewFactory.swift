import Products
import TrUAPIHost
import UIKit
import UIKitExt

/// The app's own scanner, titled for the product asking and reading only the
/// formats it named.
@MainActor
enum ProductScanViewFactory {
    static func createView(
        productId: ProductId,
        request: HostScannerScanRequest,
        answer: @escaping (HostScan) -> Void
    ) -> ControllerBackedProtocol {
        let processingQueue = QRCaptureService.processingQueue
        let presenter = ProductScanPresenter(
            productId: productId,
            request: request,
            wireframe: QRScannerWireframe(),
            qrScanService: QRCaptureService(
                delegate: nil,
                delegateQueue: processingQueue,
                metadataObjectTypes: ProductScanFormats.metadataTypes(for: request.formats)
            ),
            qrExtractionService: QRExtractionService(processingQueue: processingQueue),
            errorDisplayFactory: QRScannerErrorDisplayFactory(),
            answer: answer
        )

        let view = QRScannerViewController(presenter: presenter)
        view.title = String(localized: .Products.scanTitle(productId: productId))
        view.messageVisibilityDuration = 2
        // Nothing of the product shows through before the camera starts.
        view.rootView.backgroundColor = .black
        if let hint = request.hint {
            view.rootView.titleLabel.text = hint
            view.rootView.titleLabel.numberOfLines = 1
        }
        presenter.view = view

        let navigationController = AppNavigationController(rootViewController: view)
        navigationController.barSettings = .scannerSettings
        // The title sits on the camera's black whatever the app's theme.
        navigationController.overrideUserInterfaceStyle = .dark
        navigationController.modalPresentationStyle = .fullScreen
        return navigationController
    }
}
