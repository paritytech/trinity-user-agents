import UIKit
import DesignSystem
import PolkadotUI
import SnapKit

final class TabBarChromeSurfaceView: UIView {
    private let glassContainer = DSGlassContainerView(
        shape: .rounded(32),
        tint: nil
    )
    private let tabsPanelView = DSTabBarTabsPanelView()
    private let contentPanelView = DSTabBarContentPanelView()

    private weak var barView: DSTabBarView?
    private var glassContainerHeightConstraint: Constraint?
    private var appliedGlassContainerHeight: CGFloat = 0
    private var restingBottomConstraints: [Constraint] = []
    private var sunkenBottomConstraints: [Constraint] = []
    private var isPanelTrackingKeyboard = false
    private var isContentFilling = false

    /// The chrome sits at the bottom, clearing the home indicator gap, and a keyboard covers it
    /// like any other bottom bar. The one exception is the chrome's own search: focusing it sinks
    /// the chrome by half a capsule, so the capsule's lower half hides behind the keys.
    private static let restingBottomOffset = -DSTabBarView.bottomGap
    private static let sunkenBottomOffset = DSTabBarView.capsuleHeight / 2

    var availablePanelHeight: CGFloat {
        let occupiedHeight: CGFloat =
            if isPanelTrackingKeyboard {
                bounds.height - keyboardLayoutGuide.layoutFrame.minY - Self.sunkenBottomOffset
            } else {
                DSTabBarView.preferredHeight()
            }

        return bounds.height - topInset - occupiedHeight
    }

    /// The chain-status strip reaches the chrome as an additional top safe-area inset. A filling
    /// panel passes under it and stops at the status bar; every other state keeps clear of it.
    private var topInset: CGFloat {
        guard isContentFilling, let windowInset = window?.safeAreaInsets.top else {
            return safeAreaInsets.top
        }
        return windowInset
    }

    var panelHeight: CGFloat {
        appliedGlassContainerHeight
    }

    var onChipTapped: ((UUID) -> Void)?
    var onChipCloseRequested: ((UUID) -> Void)?

    override init(frame: CGRect) {
        super.init(frame: frame)

        installGlassContainer()
        installTabsPanel()
        installContentPanel()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hitView = super.hitTest(point, with: event)
        return hitView === self ? nil : hitView
    }

    func addBar(_ bar: DSTabBarView) {
        addSubview(bar)
        barView = bar

        bar.snp.makeConstraints { make in
            make.leading.trailing.equalTo(glassContainer.contentView)
            make.height.equalTo(DSTabBarView.capsuleHeight)
            pinBottom(make)
        }
    }

    func setPanelsOpen(_ kind: TabBarPanelKind?, animator: UIViewPropertyAnimator?) {
        tabsPanelView.setOpen(kind == .spaTabs, animator: animator)
        contentPanelView.setOpen(kind?.contentAction != nil, animator: animator)

        // Only an open panel can hold the focused search the sunken anchor belongs to.
        if kind == nil {
            setPanelTracksKeyboard(false)
        }
    }

    @discardableResult
    func updateHeight(for kind: TabBarPanelKind?, animator: UIViewPropertyAnimator?) -> Bool {
        let containerHeight: CGFloat =
            switch kind {
            case .spaTabs:
                tabsPanelView.preferredHeight(availableHeight: availablePanelHeight)
            case .content:
                // While a search is active the content fills the space above the keys or the
                // tab bar instead of fitting its rows.
                if isContentFilling {
                    max(0, availablePanelHeight)
                } else {
                    contentPanelView.preferredHeight(availableHeight: availablePanelHeight)
                }
            case nil:
                DSTabBarView.capsuleHeight
            }

        guard containerHeight != appliedGlassContainerHeight else {
            return false
        }

        appliedGlassContainerHeight = containerHeight
        glassContainerHeightConstraint?.update(offset: containerHeight)

        animator?.addAnimations { [weak self] in
            self?.layoutIfNeeded()
        }

        return true
    }

    /// Sets the panel height for interactive drag. Keeps `appliedGlassContainerHeight` in step
    /// so a later `updateHeight` still sees a change.
    func setPanelHeight(_ height: CGFloat) {
        appliedGlassContainerHeight = height
        glassContainerHeightConstraint?.update(offset: height)
        layoutIfNeeded()
    }

    func setPanelContentClipped(_ clipped: Bool) {
        contentPanelView.setContentClipped(clipped)
    }

    func setChips(_ chips: [DSTabBarChip], selected: UUID?, closeActionTitle: String) {
        tabsPanelView.setChips(chips, selected: selected)
        tabsPanelView.closeActionTitle = closeActionTitle
    }

    func setContentConfiguration(_ configuration: (any HashableContentConfiguration)?) {
        contentPanelView.setConfiguration(configuration)
    }

    func setContentHostedView(_ view: UIView?) {
        contentPanelView.setHostedView(view)
    }

    func setPanelTracksKeyboard(_ tracking: Bool) {
        guard tracking != isPanelTrackingKeyboard else {
            return
        }

        isPanelTrackingKeyboard = tracking

        if tracking {
            restingBottomConstraints.forEach { $0.deactivate() }
            sunkenBottomConstraints.forEach { $0.activate() }
        } else {
            sunkenBottomConstraints.forEach { $0.deactivate() }
            restingBottomConstraints.forEach { $0.activate() }
        }

        barView?.setKeyboardShadowVisible(tracking)
    }

    /// While a search is active the content panel fills the available height instead of fitting its rows.
    func setContentFillsAvailableHeight(_ fills: Bool) {
        isContentFilling = fills
    }
}

// MARK: - Layout

private extension TabBarChromeSurfaceView {
    /// Pins a view to the chrome's bottom, and prepares the anchor it swaps to while the chrome's
    /// own search is focused. Only that explicit focus moves the chrome, so no keyboard raised by
    /// another screen can reach it.
    func pinBottom(_ make: ConstraintMaker) {
        restingBottomConstraints.append(
            make.bottom.equalToSuperview().offset(Self.restingBottomOffset).constraint
        )

        let sunken = make.bottom.equalTo(keyboardLayoutGuide.snp.top)
            .offset(Self.sunkenBottomOffset).constraint
        sunken.deactivate()
        sunkenBottomConstraints.append(sunken)
    }

    func installGlassContainer() {
        insertSubview(glassContainer, at: 0)
        glassContainer.snp.makeConstraints { make in
            make.centerX.equalToSuperview()
            make.width.lessThanOrEqualTo(DSTabBarView.maxWidth)
            make.width.equalToSuperview().offset(-DSTabBarView.horizontalMargin * 2).priority(.high)
            pinBottom(make)
            glassContainerHeightConstraint = make.height.equalTo(DSTabBarView.capsuleHeight).constraint
        }
    }

    func installTabsPanel() {
        installPanel(tabsPanelView)

        tabsPanelView.onChipTapped = { [weak self] id in self?.onChipTapped?(id) }
        tabsPanelView.onChipCloseRequested = { [weak self] id in self?.onChipCloseRequested?(id) }
    }

    func installContentPanel() {
        installPanel(contentPanelView)
    }

    /// Both panels fill the glass above the capsule, which stays uncovered at the bottom,
    /// and always stop a capsule short of the glass bottom, at rest and over the keyboard alike.
    func installPanel(_ panel: UIView) {
        addSubview(panel)

        panel.snp.makeConstraints { make in
            make.top.leading.trailing.equalTo(glassContainer.contentView)
            make.bottom.equalTo(glassContainer.contentView)
                .offset(-DSTabBarView.capsuleHeight)
        }
    }
}
