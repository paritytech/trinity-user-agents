import UIKit

public extension CALayer {
    /// Excludes the layer from screenshots, screen recordings and mirroring.
    /// Relies on private UIKit behaviour of secure text fields and does nothing if that behaviour changes.
    @MainActor
    func setScreenCaptureDisabled(_ disabled: Bool) {
        SecureTextCanvas.apply(disabled, to: self)
    }
}

@MainActor
private enum SecureTextCanvas {
    static let textField = UITextField()

    static let canvasView: UIView? = {
        let canvasView = textField.subviews.first {
            NSStringFromClass(type(of: $0)).contains("TextLayoutCanvasView")
        }
        assert(canvasView != nil, "Secure text canvas not found, screen capture protection is off")
        return canvasView
    }()

    static func apply(_ disabled: Bool, to layer: CALayer) {
        guard let canvasView else {
            return
        }

        let originalLayer = canvasView.layer
        canvasView.setValue(layer, forKey: "layer")
        textField.isSecureTextEntry = !disabled
        textField.isSecureTextEntry = disabled
        canvasView.setValue(originalLayer, forKey: "layer")
    }
}
