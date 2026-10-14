import Foundation

final class URLScanPresenter: QRScannerPresenter {
    weak var delegate: URLScanDelegate?
    private let initialMessage: String?
    private var lastHandledCode: String?

    init(
        wireframe: QRScannerWireframeProtocol,
        errorDisplayFactory: QRScannerErrorDisplayFactoryProtocol,
        delegate: URLScanDelegate,
        qrScanService: QRCaptureServiceProtocol,
        qrExtractionService: QRExtractionServiceProtocol,
        initialMessage: String? = nil,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.delegate = delegate
        self.initialMessage = initialMessage

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

    override func setup() {
        super.setup()

        if let initialMessage {
            view?.present(message: initialMessage, animated: true, autoDismiss: false)
        }
    }

    override func handle(code: String) {
        guard lastHandledCode != code else {
            return
        }

        lastHandledCode = code

        if let url = URL(string: code) {
            DispatchQueue.main.async { [weak self] in
                MainActor.assumeIsolated {
                    self?.delegate?.urlScanDidReceiveResult(url)
                }
            }
        } else {
            DispatchQueue.main.async { [weak self] in
                self?.handleFailure()
            }
        }
    }
}
