import Foundation

@MainActor
enum AddressScanViewFactory {
    static func createView(
        for delegate: AddressScanDelegate,
        context: AnyObject?
    ) -> QRScannerViewProtocol? {
        let matcher = AddressQRMatcher()

        let processingQueue = QRCaptureService.processingQueue
        let qrService = QRCaptureService(delegate: nil, delegateQueue: processingQueue)
        let qrExtractor = QRExtractionService(processingQueue: processingQueue)

        let wireframe = QRScannerWireframe()

        let presenter = AddressScanPresenter(
            matcher: matcher,
            wireframe: wireframe,
            errorDisplayFactory: QRScannerErrorDisplayFactory(),
            delegate: delegate,
            context: context,
            qrScanService: qrService,
            qrExtractionService: qrExtractor
        )

        let view = QRScannerViewController(presenter: presenter)

        presenter.view = view

        return view
    }
}
