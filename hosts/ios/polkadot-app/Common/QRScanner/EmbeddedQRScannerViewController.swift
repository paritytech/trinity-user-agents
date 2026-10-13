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
    /// A session reported after capture was switched off is about to stop, so it is not attached.
    private var isCaptureActive = true

    override func loadView() {
        view = EmbeddedQRScannerViewLayout()
    }

    override func viewWillDisappear(_ animated: Bool) {
        detachPreview()
        super.viewWillDisappear(animated)
    }

    /// The embedded preview lives in `CameraPreviewView`, not in the frame view's bare layer.
    override func configureVideoLayer(with captureSession: AVCaptureSession) {
        guard isCaptureActive,
              let layout = view as? EmbeddedQRScannerViewLayout,
              let previewLayer = layout.previewView.previewLayer else {
            return
        }

        if previewLayer.session !== captureSession {
            previewLayer.session = captureSession
        }

        layout.didAttachPreview()
    }

    /// Detaches the preview before the stop is queued: attaching or detaching a preview layer reconfigures the
    /// session on the main thread, and `stopRunning` on the capture queue throws if it lands inside that.
    func setCaptureActive(_ active: Bool) {
        isCaptureActive = active

        if !active {
            detachPreview()
            layout?.hidePreview()
        }

        presenter.setCaptureActive(active)
    }
}

// MARK: - Private

private extension EmbeddedQRScannerViewController {
    var layout: EmbeddedQRScannerViewLayout? {
        viewIfLoaded as? EmbeddedQRScannerViewLayout
    }

    func detachPreview() {
        guard let previewLayer = layout?.previewView.previewLayer, previewLayer.session != nil else {
            return
        }
        previewLayer.session = nil
    }
}
