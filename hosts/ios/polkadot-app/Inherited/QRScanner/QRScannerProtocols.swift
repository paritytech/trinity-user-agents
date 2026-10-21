import Foundation
import AVFoundation
import UIKitExt

protocol QRScannerViewProtocol: ControllerBackedProtocol {
    var isCoveredByModal: Bool { get }
    func didReceive(session: AVCaptureSession)
    /// Attaches the preview to a session that has not started yet, without showing it.
    func attachPreview(to session: AVCaptureSession)
    func present(message: String, animated: Bool, autoDismiss: Bool)
}

extension QRScannerViewProtocol {
    func present(message: String, animated: Bool) {
        present(message: message, animated: animated, autoDismiss: true)
    }
}

@MainActor
protocol QRScannerWireframeProtocol {
    func askOpenSettings(from view: QRScannerViewProtocol?)
    /// The user has just declined camera access in the system prompt.
    func didDenyCameraAccess(from view: QRScannerViewProtocol?)
}

extension QRScannerWireframeProtocol {
    func didDenyCameraAccess(from _: QRScannerViewProtocol?) {}
}

@MainActor
protocol QRScannerPresenterProtocol: AnyObject {
    func setup()
    func viewDidAppear()
    func viewWillDisappear()
    func setCaptureActive(_ active: Bool)
}
