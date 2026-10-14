import Foundation

final class AddressScanPresenter: QRScannerPresenter {
    let matcher: AddressQRMatching
    let context: AnyObject?

    weak var delegate: AddressScanDelegate?

    private var lastHandledCode: String?

    init(
        matcher: AddressQRMatching,
        wireframe: QRScannerWireframeProtocol,
        errorDisplayFactory: QRScannerErrorDisplayFactoryProtocol,
        delegate: AddressScanDelegate,
        context: AnyObject?,
        qrScanService: QRCaptureServiceProtocol,
        qrExtractionService: QRExtractionServiceProtocol,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.matcher = matcher
        self.delegate = delegate
        self.context = context

        super.init(
            wireframe: wireframe,
            qrScanService: qrScanService,
            qrExtractionService: qrExtractionService,
            errorDisplayFactory: errorDisplayFactory,
            logger: logger
        )
    }

    private func handleFailure() {
        let message = errorDisplayFactory.createMatcherFailedString()
        view?.present(message: message, animated: true)
    }

    override func handle(code: String) {
        guard lastHandledCode != code else {
            return
        }

        lastHandledCode = code

        if let address = matcher.match(code: code) {
            DispatchQueue.main.async { [weak self] in
                self?.delegate?.addressScanDidReceiveRecepient(address: address, context: self?.context)
            }
        } else {
            DispatchQueue.main.async { [weak self] in
                self?.handleFailure()
            }
        }
    }
}
