import UIKit
import PolkadotUI
import SnapKit

final class TabBarChromeSurfaceView: UIView {
    private let panelContainer = TabBarPanelContainerView()
    private let barContainer = TabBarBarHostView()

    private var barHeightConstraint: Constraint?
    private var barHostHeight = DSTabBarStyle.height

    /// The chrome sits at the bottom, clearing the home indicator gap, and a keyboard covers it
    /// like any other bottom bar.
    private static let restingBottomOffset = -DSTabBarStyle.bottomGap

    /// The distance the chrome is raised by the keyboard. It shrinks the room left above the bar.
    var keyboardLift: CGFloat = 0 {
        didSet {
            guard keyboardLift != oldValue else {
                return
            }
            setNeedsLayout()
        }
    }

    var availablePanelHeight: CGFloat {
        max(0, bounds.height - safeAreaInsets.top - DSTabBarStyle.restingHeight - keyboardLift)
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        installPanelContainer()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        refreshBarHostHeight()
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
            make.leading.trailing.equalTo(panelContainer)
            barHeightConstraint = make.height.equalTo(barHostHeight).constraint
            pinBottom(make)
        }
    }

    /// Sets the region at the bottom that accepts touches. The container's own height does not change.
    func setBarLiveHeight(_ liveHeight: CGFloat) {
        barContainer.liveHeight = liveHeight
    }
}

// MARK: - Layout

private extension TabBarChromeSurfaceView {
    /// Pins a view to the chrome's resting bottom.
    func pinBottom(_ make: ConstraintMaker) {
        make.bottom.equalToSuperview().offset(Self.restingBottomOffset)
    }

    /// Only a change in the available height reaches the constraint. A panel opening leaves it alone,
    /// so the hosted view's coordinate space stays still during the animation.
    func refreshBarHostHeight() {
        let height = DSTabBarStyle.height + max(0, availablePanelHeight)
        guard height != barHostHeight else {
            return
        }
        barHostHeight = height
        barHeightConstraint?.update(offset: height)
    }

    func installPanelContainer() {
        insertSubview(panelContainer, at: 0)
        panelContainer.snp.makeConstraints { make in
            make.centerX.equalToSuperview()
            make.width.lessThanOrEqualTo(DSTabBarStyle.maxWidth)
            make.width.equalToSuperview().offset(-DSTabBarStyle.horizontalMargin * 2).priority(.high)
            pinBottom(make)
            make.height.equalTo(DSTabBarStyle.height)
        }
    }
}

/// Holds the panels above the bar row. It passes touches through wherever no panel is, so the
/// content behind the chrome stays reachable.
private final class TabBarPanelContainerView: UIView {
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hitView = super.hitTest(point, with: event)
        return hitView === self ? nil : hitView
    }
}
