import DesignSystem
import UIKit

/// The search field material: a 6 % fill, an 18 % top line and a 1 pt 8 % outline, in the inverted surface colour.
final class DSTabBarFieldMaterialView: UIView {
    private static let fillAlpha: CGFloat = 0.06
    private static let outlineAlpha: CGFloat = 0.08
    private static let topLineAlpha: CGFloat = 0.18

    private let cornerRadius: CGFloat
    private let topLineLayer = CAShapeLayer()
    private let topLineMask = CAGradientLayer()

    init(cornerRadius: CGFloat) {
        self.cornerRadius = cornerRadius
        super.init(frame: .zero)
        setupLayers()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        updateTopLine()
    }
}

private extension DSTabBarFieldMaterialView {
    func setupLayers() {
        isUserInteractionEnabled = false
        layer.cornerRadius = cornerRadius
        layer.cornerCurve = .continuous
        layer.borderWidth = 1
        applyColors()
        registerForTraitChanges([DSThemeTrait.self, UITraitUserInterfaceStyle.self]) {
            (view: DSTabBarFieldMaterialView, _) in
            view.applyColors()
        }

        topLineLayer.fillColor = UIColor.clear.cgColor
        topLineLayer.lineWidth = 1

        topLineMask.colors = [UIColor.black.cgColor, UIColor.clear.cgColor]
        topLineMask.locations = [0, 0.3]
        topLineMask.startPoint = CGPoint(x: 0.5, y: 0)
        topLineMask.endPoint = CGPoint(x: 0.5, y: 1)
        topLineLayer.mask = topLineMask

        layer.addSublayer(topLineLayer)
    }

    /// CGColor values do not follow trait changes, so each colour is resolved against the current traits.
    func applyColors() {
        let inverted = UIColor.bgSurfaceContainerInverted
        backgroundColor = inverted.withAlphaComponent(Self.fillAlpha).resolvedColor(with: traitCollection)
        layer.borderColor = inverted.withAlphaComponent(Self.outlineAlpha).resolvedColor(with: traitCollection).cgColor
        topLineLayer.strokeColor = inverted.withAlphaComponent(Self.topLineAlpha).resolvedColor(with: traitCollection)
            .cgColor
    }

    func updateTopLine() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        topLineLayer.frame = bounds
        topLineMask.frame = bounds
        topLineLayer.path = UIBezierPath(
            roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5),
            cornerRadius: max(0, cornerRadius - 0.5)
        ).cgPath
        CATransaction.commit()
    }
}
