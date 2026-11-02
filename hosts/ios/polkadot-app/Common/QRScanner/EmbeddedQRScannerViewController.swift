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
    private var stopWait: Task<Void, Never>?

    override func loadView() {
        view = EmbeddedQRScannerViewLayout()
    }

    override func viewWillDisappear(_ animated: Bool) {
        if stopWait == nil {
            detachPreviewAfterStop()
        }
        super.viewWillDisappear(animated)
    }

    /// The embedded preview lives in `CameraPreviewView`, not in the frame view's bare layer.
    override func configureVideoLayer(with captureSession: AVCaptureSession) {
        guard isCaptureActive, let layout else {
            return
        }

        attachPreview(to: captureSession)
        layout.didAttachPreview()
    }

    /// The embedded preview lives in `CameraPreviewView`; it attaches while the session is stopped.
    override func attachPreview(to session: AVCaptureSession) {
        guard isCaptureActive, let previewLayer = layout?.previewView.previewLayer,
              previewLayer.session !== session else {
            return
        }

        previewLayer.session = session
    }

    /// Pausing keeps the preview on the session: attaching or detaching a running session reconfigures it on the
    /// main thread for about 200 ms, and `stopRunning` on the capture queue throws if it lands inside that. A
    /// restart therefore finds the layer attached. The preview detaches once, when the view disappears, after the
    /// session has stopped.
    func setCaptureActive(_ active: Bool) {
        isCaptureActive = active

        if active {
            stopWait?.cancel()
            stopWait = nil
        } else {
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

    /// The wait holds the layer, so the detach still happens if this controller is released first.
    func detachPreviewAfterStop() {
        guard let previewLayer = layout?.previewView.previewLayer, let session = previewLayer.session else {
            return
        }
        guard session.isRunning else {
            detachPreview()
            return
        }
        let stops = NotificationCenter.default.notifications(
            named: AVCaptureSession.didStopRunningNotification,
            object: session
        )
        stopWait = Task { [weak self, previewLayer] in
            for await _ in stops {
                break
            }
            guard !Task.isCancelled else {
                return
            }
            self?.stopWait = nil
            previewLayer.session = nil
        }
    }
}
