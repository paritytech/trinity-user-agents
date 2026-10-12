import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// The scanner answers the product exactly once, and only after it has closed,
/// so the product's next scan finds its page on screen again.
@MainActor
struct ProductScanPresenterTests {
    private let receipt = CapturedCode(type: .qr, text: "https://greenmarket.example/r/BAG6")
    private let poster = CapturedCode(type: .qr, text: "https://elsewhere.example/poster")

    @Test("A code for another product says so and scanning goes on")
    func refusedCodeKeepsScanning() async throws {
        let scanner = try await OpenScanner()

        scanner.presenter.handle(codes: [poster])

        #expect(scanner.view.rootView.messageLabel.text == "This code isn't for greenmarket.dot")
        #expect(scanner.answers.scans.isEmpty)
        #expect(scanner.view.presentingViewController != nil)
    }

    @Test("Any accepted code in a frame answers, once, after the scanner has closed")
    func acceptedCodeAnswersOnceAfterClosing() async throws {
        let scanner = try await OpenScanner()

        scanner.presenter.handle(codes: [poster, receipt])
        try await waitUntil { !scanner.answers.scans.isEmpty }
        scanner.presenter.handle(codes: [receipt])

        #expect(scanner.answers.scans == [.scanned(text: "https://greenmarket.example/r/BAG6", format: .qr)])
        #expect(scanner.answers.openWhenAnswered == [false])
    }

    @Test("A camera the app cannot use answers CameraUnavailable")
    func unusableCameraAnswersCameraUnavailable() async throws {
        for error in [QRCaptureServiceError.deviceAccessRestricted, .deviceAccessDeniedNow] {
            let scanner = try await OpenScanner()

            scanner.presenter.handleQRService(error: error)
            try await waitUntil { !scanner.answers.scans.isEmpty }

            #expect(scanner.answers.scans == [.cameraUnavailable])
        }
    }

    @Test("A camera refused for good shows the settings alert before answering")
    func refusedCameraShowsSettingsFirst() async throws {
        let scanner = try await OpenScanner()

        scanner.presenter.handleQRService(error: QRCaptureServiceError.deviceAccessDeniedPreviously)
        let closeAlert = try #require(scanner.wireframe.closeAlert)
        #expect(scanner.answers.scans.isEmpty)

        closeAlert(.closed)
        try await waitUntil { !scanner.answers.scans.isEmpty }

        #expect(scanner.answers.scans == [.cameraUnavailable])
    }

    @Test("A scanner closed without a code answers Dismissed, without waiting for a message to fade")
    func closedScannerAnswersDismissed() async throws {
        let answers = Answers()
        do {
            let scanner = try await OpenScanner(answers: answers)
            scanner.presenter.handle(codes: [poster])
            scanner.view.dismiss(animated: false)
        }

        try await waitUntil { !answers.scans.isEmpty }

        #expect(answers.scans == [.dismissed])
    }
}

private final class Answers {
    var scans: [HostScan] = []
    var openWhenAnswered: [Bool] = []
}

@MainActor
private final class StubWireframe: QRScannerWireframeProtocol {
    var closeAlert: ((ApplicationSettingsAlertResult) -> Void)?

    func askOpenSettings(from _: QRScannerViewProtocol?, completion: ((ApplicationSettingsAlertResult) -> Void)?) {
        closeAlert = completion
    }
}

private final class StubCaptureService: QRCaptureServiceProtocol {
    weak var delegate: QRCaptureServiceDelegate?
    var delegateQueue = DispatchQueue.main

    func start() {}
    func stop() {}
}

/// The scanner as the factory wires it, over a test window, with the camera
/// and the settings alert stubbed.
@MainActor
private struct OpenScanner {
    let presenter: ProductScanPresenter
    let view: QRScannerViewController
    let wireframe: StubWireframe
    let answers: Answers
    let window: UIWindow

    init(answers: Answers = Answers()) async throws {
        self.answers = answers
        let window = try makeTestWindow(root: UIViewController())
        let wireframe = StubWireframe()
        presenter = ProductScanPresenter(
            productId: "greenmarket.dot",
            request: HostScannerScanRequest(formats: [.qr], prefix: "https://greenmarket.example/r/", hint: nil),
            wireframe: wireframe,
            qrScanService: StubCaptureService(),
            qrExtractionService: QRExtractionService(processingQueue: .main),
            errorDisplayFactory: QRScannerErrorDisplayFactory(),
            answer: { [answers, weak window] scan in
                answers.scans.append(scan)
                answers.openWhenAnswered.append(window?.rootViewController?.presentedViewController != nil)
            }
        )
        view = QRScannerViewController(presenter: presenter)
        presenter.view = view
        self.wireframe = wireframe
        self.window = window
        try await present(view, over: #require(window.rootViewController))
    }
}
