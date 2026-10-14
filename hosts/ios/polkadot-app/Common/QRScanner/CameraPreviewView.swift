import AVFoundation
import UIKit

/// View-backed capture preview: as a view's own layer it follows UIKit animations, where a bare
/// sublayer would need explicit layer animations to keep up.
final class CameraPreviewView: UIView {
    // swiftlint:disable:next static_over_final_class
    override class var layerClass: AnyClass {
        AVCaptureVideoPreviewLayer.self
    }

    var previewLayer: AVCaptureVideoPreviewLayer? {
        layer as? AVCaptureVideoPreviewLayer
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        previewLayer?.videoGravity = .resizeAspectFill
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}
