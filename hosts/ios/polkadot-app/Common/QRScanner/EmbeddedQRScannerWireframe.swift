import Foundation

/// Embedded scanners open every time the tab bar panel opens, so a modal alert per open
/// is unacceptable. The message goes on the panel's label instead.
@MainActor
final class EmbeddedQRScannerWireframe: QRScannerWireframeProtocol {
    func askOpenSettings(from view: QRScannerViewProtocol?) {
        view?.present(
            message: String(localized: .QRScan.errorCameraDenied),
            animated: true,
            autoDismiss: false
        )
    }

    func didDenyCameraAccess(from view: QRScannerViewProtocol?) {
        askOpenSettings(from: view)
    }
}
