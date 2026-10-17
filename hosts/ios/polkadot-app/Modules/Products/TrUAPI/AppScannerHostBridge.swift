import AVFoundation
import Foundation
import Products
import TrUAPIHost
import UIKit
import UIKitExt

/// Draws the viewfinder of the app for `scanner.scan`. The viewfinder reads the formats of the request and takes the
/// first code that the `ScanFilter` of the core accepts. The product gets that code only, never camera frames. The
/// core already checked the request and the tap of a Worker.
final class AppScannerHostBridge: ScannerHostBridge, @unchecked Sendable {
    private let present: @MainActor (ProductScanViewfinder) -> Bool

    init(routerFacade: ProductRoutersFacadeProtocol) {
        present = { viewfinder in routerFacade.productsRouter.present(view: viewfinder) }
    }

    init(present: @escaping @MainActor (ProductScanViewfinder) -> Bool) {
        self.present = present
    }

    func scanCode(
        productId: String,
        executionKind _: ProductExecutionKind,
        request: HostScannerScanRequest
    ) async throws -> HostScan {
        let session = await ProductScanSession()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                Task { @MainActor [present] in
                    guard session.start(continuation) else { return }
                    let viewfinder = ProductScanViewfinder(product: productId, request: request, session: session)
                    session.viewfinder = viewfinder
                    if !present(viewfinder) {
                        session.finish(.notVisible)
                    }
                }
            }
        } onCancel: {
            Task { @MainActor in session.cancel() }
        }
    }
}

/// The viewfinder that a product opens: the scanner screen of the app in a full-screen navigation controller with a
/// close button. The title names the product. The hint of the product shows as one line under the frame.
final class ProductScanViewfinder: AppNavigationController, ControllerBackedProtocol {
    let presenter: ProductScanPresenter

    #if targetEnvironment(simulator)
        /// The open viewfinder, for the E2E harness.
        weak static var current: ProductScanViewfinder?
    #endif

    @MainActor
    init(product: String, request: HostScannerScanRequest, session: ProductScanSession) {
        let captureService = QRCaptureService(
            delegate: nil,
            delegateQueue: QRCaptureService.processingQueue,
            codeTypes: request.formats.flatMap(\.metadataTypes)
        )
        presenter = ProductScanPresenter(
            product: product,
            request: request,
            session: session,
            captureService: captureService,
            wireframe: QRScannerWireframe()
        )
        let scanner = QRScannerViewController(presenter: presenter)
        scanner.title = String(localized: .ChatExtension.productScanTitle(product: product))
        presenter.view = scanner
        super.init(rootViewController: scanner)

        barSettings = .init(
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
        modalPresentationStyle = .fullScreen
        #if targetEnvironment(simulator)
            Self.current = self
        #endif
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

/// Runs the camera for one scan of a product and gives the answer to the session: the accepted code, a dismissal
/// when the user closes the viewfinder, or `CameraUnavailable`.
@MainActor
final class ProductScanPresenter: QRScannerPresenterProtocol {
    weak var view: QRScannerViewProtocol?

    private let product: String
    private let hint: String?
    private let filter: ScanFilter
    private let session: ProductScanSession
    private let captureService: QRCaptureServiceProtocol
    private let wireframe: QRScannerWireframe
    private var isRunning = false

    init(
        product: String,
        request: HostScannerScanRequest,
        session: ProductScanSession,
        captureService: QRCaptureServiceProtocol,
        wireframe: QRScannerWireframe
    ) {
        self.product = product
        hint = request.hint
        filter = ScanFilter(request: request)
        self.session = session
        self.captureService = captureService
        self.wireframe = wireframe
        captureService.delegate = self
    }

    func setup() {
        setCaptureActive(true)
        if let hint {
            view?.present(message: hint, animated: false, autoDismiss: false)
        }
    }

    func viewDidAppear() {
        setCaptureActive(true)
    }

    /// The viewfinder closes: a code that came first stays the answer, else the user dismissed it.
    func viewWillDisappear() {
        setCaptureActive(false)
        session.finish(.dismissed)
    }

    func setCaptureActive(_ active: Bool) {
        guard active != isRunning else { return }
        isRunning = active
        if active {
            captureService.start()
        } else {
            captureService.stop()
        }
    }

    /// A code that the camera read, with its type.
    func handle(code: String, type: AVMetadataObject.ObjectType) {
        guard let format = CodeFormat(metadataType: type) else { return }
        switch filter.observe(format: format, text: code) {
        case .accept:
            session.finish(.scanned(text: code, format: format))
            close()
        case .notForThisProduct:
            view?.present(
                message: String(localized: .ChatExtension.productScanNotForProduct(product: product)),
                animated: true
            )
        case .ignore:
            break
        }
    }

    /// The camera did not start. A refusal of the user ends the scan at once. A refusal from before, or a device
    /// without a camera, first tells the user how to turn the camera on.
    func handle(error: Error) {
        guard let captureError = error as? QRCaptureServiceError, captureError != .unsupportedFormat else { return }
        #if targetEnvironment(simulator)
            // The simulator has no camera. The viewfinder stays open, so the E2E harness can give it a code.
            if captureError == .deviceAccessRestricted {
                view?.present(message: "No camera on the simulator", animated: false, autoDismiss: false)
                return
            }
        #endif
        guard captureError != .deviceAccessDeniedNow else {
            endWithoutCamera()
            return
        }
        wireframe.askOpenApplicationSettings(
            with: String(localized: .QRScan.errorCameraRestrictedPreviously),
            title: String(localized: .QRScan.errorCameraTitle),
            from: view
        ) { [weak self] _ in
            self?.endWithoutCamera()
        }
    }

    private func endWithoutCamera() {
        session.finish(.cameraUnavailable)
        close()
    }

    private func close() {
        setCaptureActive(false)
        session.closeViewfinder()
    }
}

extension ProductScanPresenter: @MainActor QRCaptureServiceDelegate {
    func qrCapture(service _: QRCaptureServiceProtocol, didSetup captureSession: AVCaptureSession) {
        DispatchQueue.main.async {
            self.view?.didReceive(session: captureSession)
        }
    }

    func qrCapture(service _: QRCaptureServiceProtocol, didReceive _: String) {}

    func qrCapture(
        service _: QRCaptureServiceProtocol,
        didReceive code: String,
        type: AVMetadataObject.ObjectType
    ) {
        DispatchQueue.main.async {
            self.handle(code: code, type: type)
        }
    }

    func qrCapture(service _: QRCaptureServiceProtocol, didFailure error: Error) {
        DispatchQueue.main.async {
            self.handle(error: error)
        }
    }
}

/// One open viewfinder. It gives its answer one time: the first of a code, a dismissal, a failure or a cancellation
/// wins.
@MainActor
final class ProductScanSession {
    weak var viewfinder: UIViewController?
    private var continuation: CheckedContinuation<HostScan, Error>?
    private var cancelled = false

    /// Keeps the way back for the answer. Gives `false`, and answers a dismissal, when the core stopped waiting
    /// already: then no viewfinder opens.
    func start(_ continuation: CheckedContinuation<HostScan, Error>) -> Bool {
        guard !cancelled else {
            continuation.resume(returning: .dismissed)
            return false
        }
        self.continuation = continuation
        return true
    }

    func finish(_ outcome: HostScan) {
        continuation?.resume(returning: outcome)
        continuation = nil
    }

    func closeViewfinder() {
        viewfinder?.dismiss(animated: true)
    }

    /// The core stopped waiting: the viewfinder closes, and the answer is a dismissal.
    func cancel() {
        cancelled = true
        closeViewfinder()
        finish(.dismissed)
    }
}

extension CodeFormat {
    /// The AVFoundation types of the codes of this format. AVFoundation reports a UPC-A code as EAN-13 with a
    /// leading zero, as the core expects.
    var metadataTypes: [AVMetadataObject.ObjectType] {
        switch self {
        case .qr: [.qr]
        case .aztec: [.aztec]
        case .dataMatrix: [.dataMatrix]
        case .pdf417: [.pdf417]
        case .ean13: [.ean13]
        case .ean8: [.ean8]
        case .upcE: [.upce]
        case .code128: [.code128]
        case .code39: [.code39, .code39Mod43]
        case .code93: [.code93]
        case .itf: [.interleaved2of5, .itf14]
        case .codabar: [.codabar]
        }
    }

    /// The format of a code that the camera read, or nil for a type that no format names.
    init?(metadataType: AVMetadataObject.ObjectType) {
        guard let format = Self.allFormats.first(where: { $0.metadataTypes.contains(metadataType) }) else {
            return nil
        }
        self = format
    }

    private static let allFormats: [CodeFormat] = [
        .qr, .aztec, .dataMatrix, .pdf417, .ean13, .ean8, .upcE, .code128, .code39, .code93, .itf, .codabar
    ]
}
