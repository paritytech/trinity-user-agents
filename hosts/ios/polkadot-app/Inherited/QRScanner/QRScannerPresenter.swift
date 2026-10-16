import Foundation
import AVFoundation

@MainActor
class QRScannerPresenter: QRScannerPresenterProtocol {
    weak var view: QRScannerViewProtocol?

    let qrScanService: QRCaptureServiceProtocol
    let qrExtractionService: QRExtractionServiceProtocol
    let errorDisplayFactory: QRScannerErrorDisplayFactoryProtocol
    let wireframe: QRScannerWireframeProtocol
    let logger: LoggerProtocol

    private var isRunning: Bool = false

    init(
        wireframe: QRScannerWireframeProtocol,
        qrScanService: QRCaptureServiceProtocol,
        qrExtractionService: QRExtractionServiceProtocol,
        errorDisplayFactory: QRScannerErrorDisplayFactoryProtocol,
        logger: LoggerProtocol
    ) {
        self.wireframe = wireframe
        self.qrScanService = qrScanService
        self.qrExtractionService = qrExtractionService
        self.errorDisplayFactory = errorDisplayFactory
        self.logger = logger

        self.qrScanService.delegate = self
    }

    deinit {
        qrScanService.stop()
    }

    private func startServiceIfNeeded() {
        guard !isRunning else {
            return
        }

        isRunning = true

        qrScanService.start()
    }

    private func stopServiceIfNeeded() {
        guard isRunning else {
            return
        }

        isRunning = false

        qrScanService.stop()
    }

    private func handleQRService(error: Error) {
        if let captureError = error as? QRCaptureServiceError {
            handleQRCaptureService(error: captureError)
        } else if let extractionError = error as? QRExtractionServiceError {
            handleQRExtractionService(error: extractionError)
        }

        logger.error("Unexpected qr service error \(error)")
    }

    private func handleQRCaptureService(error: QRCaptureServiceError) {
        guard let view else {
            return
        }

        switch error {
        case .deviceAccessRestricted:
            view.present(
                message: errorDisplayFactory.createStringCapture(error: error),
                animated: true,
                autoDismiss: false
            )
        case .deviceAccessDeniedPreviously:
            wireframe.askOpenSettings(from: view)
        case .unsupportedFormat:
            view.present(
                message: errorDisplayFactory.createStringCapture(error: error),
                animated: true
            )
        default:
            break
        }
    }

    private func handleQRExtractionService(error: QRExtractionServiceError) {
        let message = errorDisplayFactory.createStringQRExtraction(error: error)
        view?.present(message: message, animated: true)
    }

    func handle(code _: String) {
        fatalError("Child presenter must override")
    }

    func setup() {
        startServiceIfNeeded()
    }

    func viewDidAppear() {
        startServiceIfNeeded()
    }

    func viewWillDisappear() {
        stopServiceIfNeeded()
    }

    func setCaptureActive(_ active: Bool) {
        if active {
            startServiceIfNeeded()
        } else {
            stopServiceIfNeeded()
        }
    }
}

extension QRScannerPresenter: @MainActor QRCaptureServiceDelegate {
    func qrCapture(service _: QRCaptureServiceProtocol, didSetup captureSession: AVCaptureSession) {
        DispatchQueue.main.async {
            self.view?.didReceive(session: captureSession)
        }
    }

    func qrCapture(service _: QRCaptureServiceProtocol, didReceive code: String) {
        DispatchQueue.main.async {
            self.handle(code: code)
        }
    }

    func qrCapture(service _: QRCaptureServiceProtocol, didFailure error: Error) {
        DispatchQueue.main.async {
            self.handleQRService(error: error)
        }
    }
}
