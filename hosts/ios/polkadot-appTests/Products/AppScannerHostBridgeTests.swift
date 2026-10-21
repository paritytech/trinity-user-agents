import AVFoundation
import Foundation
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// A product asks the host to scan one code. The viewfinder of the app gives the product the first code that the
/// core's filter accepts, and nothing else: a dismissal when the user closes it, and `CameraUnavailable` when the
/// app has no camera.
@MainActor
struct AppScannerHostBridgeTests {
    private let roomLinks = HostScannerScanRequest(
        formats: [.qr],
        prefix: "https://verbosity.dot.li/",
        hint: "Point at a contact link"
    )

    @Test func theFirstCodeThatTheFilterAcceptsIsTheAnswer() async throws {
        let scan = makeScan()

        let outcome = try await scan.answer {
            scan.presenter.setup()
            scan.presenter.handle(code: "https://verbosity.dot.li/-/chat/verbosity#@bob", type: .qr)
            scan.presenter.handle(code: "https://verbosity.dot.li/-/chat/verbosity#@carol", type: .qr)
        }

        #expect(outcome == .scanned(text: "https://verbosity.dot.li/-/chat/verbosity#@bob", format: .qr))
        #expect(scan.view.messages == ["Point at a contact link"])
    }

    @Test func aCodeForAnotherProductKeepsTheViewfinderOpenUntilTheUserClosesIt() async throws {
        let scan = makeScan()

        let outcome = try await scan.answer {
            scan.presenter.setup()
            scan.presenter.handle(code: "https://example.com/a", type: .qr)
            scan.presenter.handle(code: "https://verbosity.dot.li/-/chat/verbosity#@bob", type: .ean13)
            #expect(scan.view.messages.count == 3)
            scan.presenter.viewWillDisappear()
        }

        #expect(outcome == .dismissed)
        #expect(!scan.capture.running)
    }

    @Test func aCameraThatTheUserRefusesEndsTheScan() async throws {
        let scan = makeScan()

        let outcome = try await scan.answer {
            scan.presenter.handle(error: QRCaptureServiceError.deviceAccessDeniedNow)
        }

        #expect(outcome == .cameraUnavailable)
    }

    @Test func aCameraRefusedBeforeEndsTheScanAfterTheSettingsQuestion() async throws {
        let scan = makeScan()

        let outcome = try await scan.answer {
            scan.presenter.handle(error: QRCaptureServiceError.deviceAccessDeniedPreviously)
        }

        #expect(outcome == .cameraUnavailable)
    }

    @Test func aScanWithNoScreenOpensNothing() async throws {
        var presented = 0
        let bridge = AppScannerHostBridge { _ in
            presented += 1
            return false
        }

        let outcome = try await bridge.scanCode(productId: "verbosity.dot", executionKind: .app, request: roomLinks)

        #expect(outcome == .notVisible)
        #expect(presented == 1)
    }

    @Test func aScanThatTheCoreStoppedBeforeTheViewfinderOpensIsADismissal() async throws {
        let session = ProductScanSession()
        session.cancel()

        let outcome = try await withCheckedThrowingContinuation { continuation in
            #expect(!session.start(continuation))
        }

        #expect(outcome == .dismissed)
    }

    @Test func theCameraReadsTheFormatsOfTheRequest() {
        #expect(CodeFormat.itf.metadataTypes == [.interleaved2of5, .itf14])
        #expect(CodeFormat(metadataType: .itf14) == .itf)
        #expect(CodeFormat(metadataType: .ean13) == .ean13)
        #expect(CodeFormat(metadataType: .face) == nil)
    }
}

/// One scan of a product, with the doubles of its screen and camera.
@MainActor
private struct Scan {
    let presenter: ProductScanPresenter
    let session: ProductScanSession
    let view: RecordingScannerView
    let capture: RecordingCapture

    /// The answer that the scan gives after `steps`.
    func answer(steps: () -> Void) async throws -> HostScan {
        try await withCheckedThrowingContinuation { continuation in
            #expect(session.start(continuation))
            steps()
        }
    }
}

private extension AppScannerHostBridgeTests {
    func makeScan() -> Scan {
        let session = ProductScanSession()
        let capture = RecordingCapture()
        let presenter = ProductScanPresenter(
            product: "verbosity.dot",
            request: roomLinks,
            session: session,
            captureService: capture,
            wireframe: QRScannerWireframe()
        )
        let view = RecordingScannerView()
        presenter.view = view
        return Scan(presenter: presenter, session: session, view: view, capture: capture)
    }
}

private final class RecordingScannerView: UIViewController, QRScannerViewProtocol {
    private(set) var messages: [String] = []

    var isCoveredByModal: Bool { false }

    func didReceive(session _: AVCaptureSession) {}

    func present(message: String, animated _: Bool, autoDismiss _: Bool) {
        messages.append(message)
    }
}

private final class RecordingCapture: QRCaptureServiceProtocol {
    weak var delegate: QRCaptureServiceDelegate?
    var delegateQueue = DispatchQueue.main
    private(set) var running = false

    func start() {
        running = true
    }

    func stop() {
        running = false
    }
}
