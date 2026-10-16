import UIKit

final class DSTabBarKeyboardShadowView: UIView {
    private enum Constants {
        static let shadowOpacity: CGFloat = 0.75
        /// The ramp starts a tenth down the capsule so its top edge stays clear.
        static let gradientStart: NSNumber = 0.1
    }

    override class var layerClass: AnyClass {
        CAGradientLayer.self
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        isUserInteractionEnabled = false
        alpha = 0

        guard let gradientLayer = layer as? CAGradientLayer else {
            return
        }

        gradientLayer.colors = [
            UIColor.black.withAlphaComponent(0).cgColor,
            UIColor.black.withAlphaComponent(Constants.shadowOpacity).cgColor
        ]
        gradientLayer.locations = [Constants.gradientStart, 1]
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}
