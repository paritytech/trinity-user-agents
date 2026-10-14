import Foundation

@MainActor
enum URLScanViewFactory {
    static func createView(
        for delegate: URLScanDelegate,
        initialMessage: String? = nil
    ) -> QRScannerViewProtocol? {
        let processingQueue = QRCaptureService.processingQueue
        let qrService = QRCaptureService(delegate: nil, delegateQueue: processingQueue)
        let qrExtractor = QRExtractionService(processingQueue: processingQueue)

        let wireframe = QRScannerWireframe()

        let presenter = URLScanPresenter(
            wireframe: wireframe,
            errorDisplayFactory: QRScannerErrorDisplayFactory(),
            delegate: delegate,
            qrScanService: qrService,
            qrExtractionService: qrExtractor,
            initialMessage: initialMessage
        )

        let view = QRScannerViewController(presenter: presenter)

        presenter.view = view

        return view
    }
}
