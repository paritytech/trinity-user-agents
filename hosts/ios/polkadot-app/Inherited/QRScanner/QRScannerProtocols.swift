import Foundation
import AVFoundation
import UIKitExt

protocol QRScannerViewProtocol: ControllerBackedProtocol {
    var isCoveredByModal: Bool { get }
    func didReceive(session: AVCaptureSession)
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
}

@MainActor
protocol QRScannerPresenterProtocol: AnyObject {
    func setup()
    func viewDidAppear()
    func viewWillDisappear()
    func setCaptureActive(_ active: Bool)
}
