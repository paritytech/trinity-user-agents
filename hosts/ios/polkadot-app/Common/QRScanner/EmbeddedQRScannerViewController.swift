import AVFoundation
import UIKit

/// What the scan panel needs from its scanner: capture stops while the collapsed preview shows
/// nothing.
protocol ScanPanelScannerControlling: AnyObject {
    func setCaptureActive(_ active: Bool)
}

/// Hosted as a child of the tab bar panel rather than presented, so it drops the framed
/// full-screen chrome. The layout still carries the message label, so presenter messages use
/// the inherited presentation.
final class EmbeddedQRScannerViewController: QRScannerViewController, ScanPanelScannerControlling {
    override func loadView() {
        view = EmbeddedQRScannerViewLayout()
    }

    /// The embedded preview lives in `CameraPreviewView`, not in the frame view's bare layer.
    override func configureVideoLayer(with captureSession: AVCaptureSession) {
        guard let layout = view as? EmbeddedQRScannerViewLayout,
              let previewLayer = layout.previewView.previewLayer else {
            return
        }

        if previewLayer.session !== captureSession {
            previewLayer.session = captureSession
        }

        layout.didAttachPreview()
    }

    func setCaptureActive(_ active: Bool) {
        presenter.setCaptureActive(active)

        guard !active else { return }
        layout?.hidePreview()
    }
}

// MARK: - Private

private extension EmbeddedQRScannerViewController {
    var layout: EmbeddedQRScannerViewLayout? {
        view as? EmbeddedQRScannerViewLayout
    }
}
