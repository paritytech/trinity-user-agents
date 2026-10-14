import DesignSystem
import UIKit

final class DSTabBarSelectionLens: UIView {
    let contentView = UIView()
    let selectedContentView = UIView()

    private let pillView: UIView
    private let pillMaskView = UIView()
    private var appliedLift: Bool?

    override init(frame: CGRect) {
        pillView = DSTabBarSelectionLens.makePill()

        super.init(frame: frame)

        isUserInteractionEnabled = false

        addSubview(pillView)
        addSubview(contentView)
        addSubview(selectedContentView)

        pillMaskView.backgroundColor = .black
        pillMaskView.layer.cornerCurve = .continuous
        pillMaskView.layer.cornerRadius = DSTabBarMetrics.pillCornerRadius
        selectedContentView.mask = pillMaskView
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()

        contentView.frame = bounds
        selectedContentView.frame = bounds
    }

    func update(pillFrame: CGRect, isLifted: Bool, animated: Bool) {
        if pillView.bounds.size != pillFrame.size {
            pillView.bounds = CGRect(origin: .zero, size: pillFrame.size)
            pillView.setNeedsLayout()
            pillView.layoutIfNeeded()
            pillMaskView.bounds = pillView.bounds
        }

        let scaleX = isLifted ? DSTabBarMetrics.liftedScaleX : 1
        let scaleY = isLifted ? DSTabBarMetrics.liftedScaleY : 1
        let centre = CGPoint(x: pillFrame.midX, y: pillFrame.midY)
        let transform = CGAffineTransform(scaleX: scaleX, y: scaleY)
        let apply = {
            self.pillView.center = centre
            self.pillView.transform = transform
            self.pillView.alpha = 1
            self.pillMaskView.center = centre
            self.pillMaskView.transform = transform
        }

        guard animated else {
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            apply()
            CATransaction.commit()
            return
        }

        UIView.animate(
            withDuration: DSTabBarMetrics.selectionSpringDuration,
            delay: 0,
            usingSpringWithDamping: DSTabBarMetrics.selectionSpringDamping,
            initialSpringVelocity: 0,
            options: [.allowUserInteraction, .beginFromCurrentState],
            animations: apply
        )
    }
}

private extension DSTabBarSelectionLens {
    /// A flat fill rather than glass: clear glass samples whatever scrolls under the bar, so the
    /// selection would drift in colour from screen to screen.
    static func makePill() -> UIView {
        let view = UIView()
        view.backgroundColor = UIColor.bgSurfaceContainerInverted.withAlphaComponent(0.12)
        view.layer.cornerCurve = .continuous
        view.layer.cornerRadius = DSTabBarMetrics.pillCornerRadius
        return view
    }
}
