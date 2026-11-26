import UIKit

/// Sweeps a highlight band across a view by masking it with an animated gradient.
public final class ShimmerMask {
    private let maskLayer = CAGradientLayer()
    private var animatedWidth: CGFloat?

    public init() {
        maskLayer.colors = [
            UIColor.white.cgColor,
            UIColor.white.withAlphaComponent(0.5).cgColor,
            UIColor.white.cgColor
        ]
        maskLayer.locations = [0.4, 0.5, 0.6]
        maskLayer.startPoint = CGPoint(x: 0, y: 0.5)
        maskLayer.endPoint = CGPoint(x: 1, y: 0.5)
    }

    public func setActive(_ isActive: Bool, on view: UIView) {
        guard isActive else {
            if view.layer.mask === maskLayer {
                view.layer.mask = nil
            }
            maskLayer.removeAllAnimations()
            animatedWidth = nil
            return
        }
        guard view.layer.mask !== maskLayer else { return }

        view.layer.mask = maskLayer
        layout(in: view)
    }

    public func layout(in view: UIView) {
        guard view.layer.mask === maskLayer else { return }

        let width = view.bounds.width
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        maskLayer.bounds = CGRect(
            x: 0,
            y: 0,
            width: width * Constants.maskSpan,
            height: view.bounds.height
        )
        maskLayer.position = CGPoint(x: view.bounds.midX, y: view.bounds.midY)
        CATransaction.commit()

        let hasSweep = maskLayer.animation(forKey: Constants.animationKey) != nil
        guard !hasSweep || animatedWidth != width else { return }

        animatedWidth = width
        maskLayer.removeAnimation(forKey: Constants.animationKey)
        let sweep = CABasicAnimation(keyPath: "position.x")
        sweep.fromValue = Constants.startPhase * width
        sweep.toValue = Constants.endPhase * width
        sweep.duration = Constants.duration
        sweep.repeatCount = .infinity
        maskLayer.add(sweep, forKey: Constants.animationKey)
    }
}

private extension ShimmerMask {
    enum Constants {
        static let animationKey = "shimmer"
        static let duration: CFTimeInterval = 1.2
        static let startPhase: CGFloat = -0.3
        static let endPhase: CGFloat = 1.3
        /// The mask spans three widths so the highlight band clears both edges while sweeping.
        static let maskSpan: CGFloat = 3
    }
}
