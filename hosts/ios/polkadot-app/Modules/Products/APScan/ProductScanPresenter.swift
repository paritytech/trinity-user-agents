import Foundation
import Products
import TrUAPIHost

/// Feeds every code the camera reads through the core's `ScanFilter` and
/// answers the product once, after the scanner has closed, so its page is on
/// screen again when it hears the answer. A scanner that goes away
/// unanswered answers `.dismissed`.
final class ProductScanPresenter: QRScannerPresenter {
    private let productId: ProductId
    private let filter: ScanFilter
    private var answer: ((HostScan) -> Void)?
    private var choice: HostScan?

    init(
        productId: ProductId,
        request: HostScannerScanRequest,
        wireframe: QRScannerWireframeProtocol,
        qrScanService: QRCaptureServiceProtocol,
        qrExtractionService: QRExtractionServiceProtocol,
        errorDisplayFactory: QRScannerErrorDisplayFactoryProtocol,
        answer: @escaping (HostScan) -> Void
    ) {
        self.productId = productId
        filter = ScanFilter(request: request)
        self.answer = answer

        super.init(
            wireframe: wireframe,
            qrScanService: qrScanService,
            qrExtractionService: qrExtractionService,
            errorDisplayFactory: errorDisplayFactory,
            logger: Logger.shared
        )
    }

    deinit {
        answer?(choice ?? .dismissed)
    }

    /// The camera starts once the scanner is up, so a camera error never
    /// closes it while it is still opening.
    override func setup() {}

    override func handle(codes: [CapturedCode]) {
        for code in codes {
            guard let text = code.text, let format = ProductScanFormats.format(of: code.type) else {
                continue
            }
            switch filter.observe(format: format, text: text) {
            case .accept:
                close(answering: .scanned(text: text, format: format))
                return
            case .notForThisProduct:
                view?.present(
                    message: String(localized: .Products.scanNotForThisProduct(productId: productId)),
                    animated: true
                )
            case .ignore:
                break
            }
        }
    }

    override func handleQRService(error: Error) {
        switch error as? QRCaptureServiceError {
        case .deviceAccessDeniedPreviously:
            wireframe.askOpenSettings(from: view) { [weak self] _ in
                self?.close(answering: .cameraUnavailable)
            }
        case .deviceAccessDeniedNow,
             .deviceAccessRestricted:
            close(answering: .cameraUnavailable)
        case .unsupportedFormat,
             nil:
            super.handleQRService(error: error)
        }
    }

    private func close(answering scan: HostScan) {
        guard choice == nil else {
            return
        }
        choice = scan
        view?.controller.dismiss(animated: true) { [weak self] in
            MainActor.assumeIsolated { self?.deliverChoice() }
        }
    }

    private func deliverChoice() {
        answer?(choice ?? .dismissed)
        answer = nil
    }
}
