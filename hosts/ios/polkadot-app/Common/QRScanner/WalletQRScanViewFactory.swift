import Foundation

@MainActor
enum WalletQRScanViewFactory {
    /// Embedded is hosted inside the tab bar panel; full screen is presented modally.
    enum Presentation {
        case fullScreen
        case embedded
    }

    static func createView(
        for delegate: WalletQRScanDelegate,
        presentation: Presentation = .fullScreen
    ) -> QRScannerViewProtocol? {
        let processingQueue = QRCaptureService.processingQueue
        let qrService = QRCaptureService(delegate: nil, delegateQueue: processingQueue)
        let qrExtractor = QRExtractionService(processingQueue: processingQueue)

        let wireframe = QRScannerWireframe()

        let presenter = WalletQRScanPresenter(
            matcher: AddressQRMatcher(),
            wireframe: wireframe,
            errorDisplayFactory: QRScannerErrorDisplayFactory(),
            delegate: delegate,
            dsfinvkParser: W3sDsfinvkReceiptParser(),
            // Allowlist confines the scanner to our own deeplinks; rejects tel:/sms:/etc.
            isAcceptedScheme: AppConfig.DeepLink.isKnownScheme,
            qrScanService: qrService,
            qrExtractionService: qrExtractor
        )

        let view: QRScannerViewController =
            switch presentation {
            case .fullScreen:
                QRScannerViewController(presenter: presenter)
            case .embedded:
                EmbeddedQRScannerViewController(presenter: presenter)
            }

        presenter.view = view

        return view
    }
}
