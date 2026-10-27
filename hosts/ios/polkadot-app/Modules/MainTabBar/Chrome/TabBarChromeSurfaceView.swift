import UIKit
import PolkadotUI
import SnapKit

final class TabBarChromeSurfaceView: UIView {
    private let barContainer = TabBarBarHostView()

    private var barHeightConstraint: Constraint?
    private var barBottomConstraint: Constraint?
    private var barHostHeight = DSTabBarStyle.height

    /// The chrome translates this view, so the gap reads the untransformed host's inset.
    private var bottomSafeAreaInset: CGFloat {
        superview?.safeAreaInsets.bottom ?? safeAreaInsets.bottom
    }

    /// The chrome sits at the bottom, clearing the home indicator gap, and a keyboard covers it
    /// like any other bottom bar.
    private var restingBottomOffset: CGFloat {
        -DSTabBarStyle.bottomGap(bottomSafeAreaInset: bottomSafeAreaInset)
    }

    /// The room above the bar at rest. The bar host is sized from this, never from the room left after a keyboard
    /// lift, which the chrome subtracts itself.
    var restingPanelHeight: CGFloat {
        let restingHeight = DSTabBarStyle.restingHeight(bottomSafeAreaInset: bottomSafeAreaInset)
        return max(0, bounds.height - safeAreaInsets.top - restingHeight)
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        refreshBarHostHeight()
    }

    override func safeAreaInsetsDidChange() {
        super.safeAreaInsetsDidChange()
        barBottomConstraint?.update(offset: restingBottomOffset)
    }

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hitView = super.hitTest(point, with: event)
        return hitView === self ? nil : hitView
    }

    /// The bar's hosted view sits in a container that stays at a constant height, so panels never resize it.
    func addBar(_ bar: UIView) {
        addSubview(barContainer)
        barContainer.addSubview(bar)

        bar.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
        barContainer.snp.makeConstraints { make in
            make.centerX.equalToSuperview()
            make.width.lessThanOrEqualTo(DSTabBarStyle.maxWidth)
            make.width.equalToSuperview().offset(-DSTabBarStyle.horizontalMargin * 2).priority(.high)
            barBottomConstraint = make.bottom.equalToSuperview().offset(restingBottomOffset).constraint
            barHeightConstraint = make.height.equalTo(barHostHeight).constraint
        }
    }

    /// Sets the region at the bottom that accepts touches. The container's own height does not change.
    func setBarLiveHeight(_ liveHeight: CGFloat) {
        barContainer.liveHeight = liveHeight
    }
}

// MARK: - Layout

private extension TabBarChromeSurfaceView {
    /// Only a change in the resting room reaches the constraint. A panel opening or the keyboard lifting leaves it
    /// alone: resizing the host mid-animation lays its SwiftUI content out at the final size at once, so the bar
    /// would snap ahead of the transform that lifts it.
    func refreshBarHostHeight() {
        let height = DSTabBarStyle.height + restingPanelHeight
        guard height != barHostHeight else {
            return
        }
        barHostHeight = height
        barHeightConstraint?.update(offset: height)
    }
}
