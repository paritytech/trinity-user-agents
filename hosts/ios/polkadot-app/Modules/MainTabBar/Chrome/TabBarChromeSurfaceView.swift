import UIKit
import DesignSystem
import PolkadotUI
import SnapKit

final class TabBarChromeSurfaceView: UIView {
    private let panelContainer = TabBarPanelContainerView()
    private let tabsPanelView = DSTabBarTabsPanelView()
    private let contentPanelView = DSTabBarContentPanelView()

    private var panelContainerHeightConstraint: Constraint?
    private var appliedPanelContainerHeight: CGFloat = 0
    private var restingBottomConstraints: [Constraint] = []
    private var sunkenBottomConstraints: [Constraint] = []
    private var isPanelTrackingKeyboard = false
    private var isContentFilling = false

    /// The chrome sits at the bottom, clearing the home indicator gap, and a keyboard covers it
    /// like any other bottom bar. The one exception is the chrome's own search: focusing it sinks
    /// the chrome by half a bar row, so the bar row's lower half hides behind the keys.
    private static let restingBottomOffset = -DSTabBarStyle.bottomGap
    private static let sunkenBottomOffset = DSTabBarStyle.height / 2

    var availablePanelHeight: CGFloat {
        let occupiedHeight: CGFloat =
            if isPanelTrackingKeyboard {
                bounds.height - keyboardLayoutGuide.layoutFrame.minY - Self.sunkenBottomOffset
            } else {
                DSTabBarStyle.restingHeight
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
        appliedPanelContainerHeight
    }

    var onChipTapped: ((UUID) -> Void)?
    var onChipCloseRequested: ((UUID) -> Void)?

    override init(frame: CGRect) {
        super.init(frame: frame)

        installPanelContainer()
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

    func addBar(_ bar: UIView) {
        addSubview(bar)

        bar.snp.makeConstraints { make in
            make.leading.trailing.equalTo(panelContainer)
            make.height.equalTo(DSTabBarStyle.height)
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
                DSTabBarStyle.height
            }

        guard containerHeight != appliedPanelContainerHeight else {
            return false
        }

        appliedPanelContainerHeight = containerHeight
        panelContainerHeightConstraint?.update(offset: containerHeight)

        animator?.addAnimations { [weak self] in
            self?.layoutIfNeeded()
        }

        return true
    }

    /// Sets the panel height for interactive drag. Keeps `appliedPanelContainerHeight` in step
    /// so a later `updateHeight` still sees a change.
    func setPanelHeight(_ height: CGFloat) {
        appliedPanelContainerHeight = height
        panelContainerHeightConstraint?.update(offset: height)
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

    func installPanelContainer() {
        insertSubview(panelContainer, at: 0)
        panelContainer.snp.makeConstraints { make in
            make.centerX.equalToSuperview()
            make.width.lessThanOrEqualTo(DSTabBarStyle.maxWidth)
            make.width.equalToSuperview().offset(-DSTabBarStyle.horizontalMargin * 2).priority(.high)
            pinBottom(make)
            panelContainerHeightConstraint = make.height.equalTo(DSTabBarStyle.height).constraint
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

    /// Both panels fill the container above the bar row, which stays uncovered at the bottom,
    /// and always stop a bar row short of the container bottom, at rest and over the keyboard alike.
    func installPanel(_ panel: UIView) {
        addSubview(panel)

        panel.snp.makeConstraints { make in
            make.top.leading.trailing.equalTo(panelContainer)
            make.bottom.equalTo(panelContainer)
                .offset(-DSTabBarStyle.height)
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
