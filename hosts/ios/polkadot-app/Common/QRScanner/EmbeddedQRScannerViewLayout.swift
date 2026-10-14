import UIKit
import UIKit_iOS
import PolkadotUI
import DesignSystem

/// Bare camera preview that fills its host's bounds: no dimmed cutout and no frame border, with
/// the message label drawn straight over the preview. `fillColor` is what draws the dimming in
/// `CameraFrameView`, so clearing it removes the window entirely. The host view (`ScanPanelViewLayout`)
/// owns the insets and placeholder.
final class EmbeddedQRScannerViewLayout: QRScannerViewLayout {
    private enum Constants {
        static let previewFadeDuration: TimeInterval = 0.25
    }

    let previewView = CameraPreviewView()
    private var previewSide: CGFloat = 0

    override func setupLayout() {
        backgroundColor = .clear

        heightAnchor.constraint(equalTo: widthAnchor).isActive = true

        addSubview(qrFrameView)
        qrFrameView.alpha = 0
        qrFrameView.layer.cornerRadius = DSRadii.large
        qrFrameView.layer.masksToBounds = true
        qrFrameView.fillColor = .clear
        qrFrameView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        qrFrameView.addSubview(previewView)

        messageLabel.textColor = .fgPrimary
        addSubview(messageLabel)
        messageLabel.snp.makeConstraints { make in
            make.leading.trailing.equalTo(qrFrameView).inset(DSSpacings.mediumIncreased)
            make.bottom.equalTo(qrFrameView).inset(DSSpacings.mediumIncreased)
        }
    }

    override func didAttachPreview() {
        UIView.animate(withDuration: Constants.previewFadeDuration) { [self] in
            qrFrameView.alpha = 1
        }
    }

    /// Drops the preview back to the placeholder while the session is stopped: a restarted session
    /// renders black until its first frame, which would otherwise blink through the expand.
    /// `didAttachPreview` fades it back when the presenter reports the restarted session.
    func hidePreview() {
        qrFrameView.alpha = 0
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        layoutPreview()
    }
}

private extension EmbeddedQRScannerViewLayout {
    /// A resized `AVCaptureVideoPreviewLayer` lays its video out at the final size immediately, pinned to
    /// the top-left, so the preview keeps the widest size it has had and is scaled around its center.
    func layoutPreview() {
        let side = qrFrameView.bounds.width

        guard side > 0 else {
            return
        }

        previewSide = max(previewSide, side)
        previewView.bounds = CGRect(x: 0, y: 0, width: previewSide, height: previewSide)
        previewView.center = CGPoint(x: qrFrameView.bounds.midX, y: qrFrameView.bounds.midY)

        let scale = side / previewSide
        previewView.transform = CGAffineTransform(scaleX: scale, y: scale)
    }
}
