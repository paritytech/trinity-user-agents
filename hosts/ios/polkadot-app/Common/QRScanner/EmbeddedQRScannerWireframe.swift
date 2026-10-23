/// Embedded scanners open every time the tab bar panel opens, so they neither raise an alert
/// nor show a message; the camera tile stays empty.
@MainActor
final class EmbeddedQRScannerWireframe: QRScannerWireframeProtocol {
    func askOpenSettings(from _: QRScannerViewProtocol?) {}

    func didDenyCameraAccess(from _: QRScannerViewProtocol?) {}
}
