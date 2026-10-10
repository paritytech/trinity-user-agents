import UIKit
import PolkadotUI
import SnapKit

final class TabBarChromeSurfaceView: UIView {
    private let panelContainer = TabBarPanelContainerView()

    private var barHeightConstraint: Constraint?
    private var appliedBarRise: CGFloat = 0

    /// The chrome sits at the bottom, clearing the home indicator gap, and a keyboard covers it
    /// like any other bottom bar.
    private static let restingBottomOffset = -DSTabBarStyle.bottomGap

    var availablePanelHeight: CGFloat {
        bounds.height - safeAreaInsets.top - DSTabBarStyle.restingHeight
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        installPanelContainer()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hitView = super.hitTest(point, with: event)
        return hitView === self ? nil : hitView
    }

    func addBar(_ bar: UIView) {
        addSubview(bar)

        bar.snp.makeConstraints { make in
            make.leading.trailing.equalTo(panelContainer)
            barHeightConstraint = make.height.equalTo(DSTabBarStyle.height).constraint
            pinBottom(make)
        }
    }

    /// Grows the bar's hosting view upward so the open centre control or shelf lies inside its bounds,
    /// where UIKit delivers touches to it. The row stays at the bottom of the hosting view.
    func setBarRise(_ rise: CGFloat) {
        guard rise != appliedBarRise else {
            return
        }
        appliedBarRise = rise
        barHeightConstraint?.update(offset: DSTabBarStyle.height + rise)
        setNeedsLayout()
    }
}

// MARK: - Layout

private extension TabBarChromeSurfaceView {
    /// Pins a view to the chrome's resting bottom.
    func pinBottom(_ make: ConstraintMaker) {
        make.bottom.equalToSuperview().offset(Self.restingBottomOffset)
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
