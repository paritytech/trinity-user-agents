import UIKit
import SwiftUI
import DesignSystem
import PolkadotUI
import SnapKit

final class TabBarBottomChromeController: UIViewController {
    /// Room the open centre panel leaves below the top safe area.
    private static let centerPanelTopGap: CGFloat = 12

    let chromeSurface = TabBarChromeSurfaceView()
    let centerButtonAnchorGuide = UILayoutGuide()
    let barModel = TabBarBarViewModel()
    lazy var barHost = UIHostingController(rootView: TabBarBarView(model: barModel))
    lazy var dimHost = UIHostingController(rootView: TabBarDimView(model: barModel))

    private var slots: [TabBarSlot] = []
    private var slotMap = TabBarSlotMap(slots: [])
    private var spaTabCount = 0
    private var selectedTabIndex = 0
    private var showsLabels = false
    private var attentionTabs: Set<TabBarItem> = []
    /// The last panel that reached `applyCommittedPanel`. By then `TabBarPanelController.open` is already the new
    /// panel.
    private var committedPanel: TabBarPanelKind?
    /// The centre panel's content, built once per open. SwiftUI may build the content more than once while
    /// measuring, and the capture session must be built only once.
    var centerPanelContent: UIViewController?
    /// The first settled layout places the bar. Animating it would spring the bar in from its unplaced frame at launch.
    private var hasPerformedInitialLayout = false
    /// The last keyboard frame in screen coordinates, kept so the lift can be re-derived when the scan panel changes.
    var keyboardFrame: CGRect?

    lazy var foldController = TabBarFoldController(
        foldSurface: chromeSurface,
        chromeBounds: { [unowned self] in view.bounds },
        bottomSafeAreaInset: { [unowned self] in view.safeAreaInsets.bottom },
        grabZoneSink: { [weak self] zone in
            (self?.viewIfLoaded as? TabBarChromePassthroughView)?.foldGrabZone = zone
        },
        closePanel: { [weak self] in
            self?.barModel.setCenterPanelOpen(false)
            self?.setPanel(nil)
        },
        stateSink: { [weak self] state in
            self?.tipController.setBarShown(state == .shown)
        }
    )

    private lazy var tipController = TabBarTipController(
        host: self,
        sequence: TabBarTipOrderedSequence(steps: TabBarTips.steps),
        centerButtonAnchor: centerButtonAnchorGuide,
        statusStripAnchor: { [weak self] in self?.statusStripAnchorProvider?() }
    )

    lazy var panelController = TabBarPanelController(
        onPanelChanged: { [weak self] kind in
            self?.applyCommittedPanel(kind)
        },
        onOpenPanelChanged: { [weak self] kind in
            self?.updateActiveActionIndex()
            self?.applyBarLiveHeight()
            self?.refreshKeyboardLift()
            (self?.viewIfLoaded as? TabBarChromePassthroughView)?.isOutsideTapEnabled = kind != nil
            if kind != nil {
                self?.tipController.dismissForPanel()
            }
        }
    )

    var onSelect: ((_ index: Int, _ isReselection: Bool) -> Void)?
    var onChipTapped: ((UUID) -> Void)?
    var onChipCloseRequested: ((UUID) -> Void)?
    var onConnectionStatusTapped: (() -> Void)?
    var onSettingsTapped: (() -> Void)?

    /// Fires as the centre panel starts closing, before the animation, so content can give up first responder
    /// and the keyboard drops in the same movement.
    var onCenterPanelWillClose: (() -> Void)?

    /// Fires once the centre panel has settled fully open or fully closed, not during the drag.
    var onCenterPanelDidChangeOpen: ((Bool) -> Void)?

    /// Fires when a swipe up on the centre button begins, before the panel is committed open.
    var onCenterPanelSwipeOpenBegan: (() -> Void)? {
        didSet {
            barModel.onCenterPanelSwipeOpenBegan = onCenterPanelSwipeOpenBegan == nil
                ? nil
                : { [weak self] in self?.onCenterPanelSwipeOpenBegan?() }
        }
    }

    /// Fires as a swipe-opened centre panel lands, once the panel is committed open, so the host can focus search.
    var onCenterPanelSwipeOpenLanding: (() -> Void)? {
        didSet {
            barModel.onCenterPanelSwipeOpenLanding = onCenterPanelSwipeOpenLanding == nil
                ? nil
                : { [weak self] in self?.centerPanelSwipeOpenLanding() }
        }
    }

    /// Builds the centre control's content. Called when the control first shows it, not when the panel commits.
    var makeCenterContent: (() -> UIViewController?)?

    /// Declares the centre content's height. The control sizes itself from this
    /// before any content exists, so it cannot be measured from the content.
    var contentHeightForCenterPanel: (() -> TabBarCenterContentHeight)?

    /// The published clearance depends on the chrome's own safe area, so the host has to
    /// recompute when that changes.
    var onBottomClearanceInvalidated: (() -> Void)?

    /// The chain-status strip is installed by `MainTabBarViewController`, not by the chrome,
    /// so its tip anchor is handed down rather than reached for.
    var statusStripAnchorProvider: (() -> (any UIPopoverPresentationControllerSourceItem)?)?

    /// Height the bar occupies at the bottom edge once it has settled. Folding and panel
    /// animations move the chrome without changing it, so the host can lay out against it.
    var restingBarHeight: CGFloat {
        guard TabBarVisibilityPolicy.contributesClearance(isTabRoot: foldController.isTabRoot) else {
            return view.safeAreaInsets.bottom
        }
        return DSTabBarStyle.restingHeight(bottomSafeAreaInset: view.safeAreaInsets.bottom)
    }

    /// The part of `restingBarHeight` the safe area does not already cover.
    var restingBottomClearance: CGFloat {
        max(0, restingBarHeight - view.safeAreaInsets.bottom)
    }

    override func loadView() {
        view = TabBarChromePassthroughView()
    }

    override func viewSafeAreaInsetsDidChange() {
        super.viewSafeAreaInsetsDidChange()

        onBottomClearanceInvalidated?()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        view.backgroundColor = .clear

        installDimLayer()
        installChromeSurface()
        installBar()

        installOutsideTapRecognizer()
        installKeyboardObserver()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)

        tipController.start()
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)

        tipController.stop()
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()

        // Order matters: the centre content height is clamped to the available height the first call writes.
        refreshAvailablePanelHeight()
        refreshCenterContentHeight()

        foldController.reapplyForWidthChange()
        if view.window != nil, view.bounds.width > 0, view.bounds.height > 0 {
            hasPerformedInitialLayout = true
            tipController.setBarLaidOut(true)
        }
    }

    func setItems(_ slots: [TabBarSlot]) {
        self.slots = slots
        rebuildItems()
    }

    func setAttention(_ isVisible: Bool, for tab: TabBarItem) {
        guard attentionTabs.contains(tab) != isVisible else {
            return
        }
        if isVisible {
            attentionTabs.insert(tab)
        } else {
            attentionTabs.remove(tab)
        }
        rebuildItems()
    }

    /// Indices crossing this boundary are tab-space; `TabBarSlotMap` converts them.
    func setSelectedIndex(_ tabIndex: Int) {
        selectedTabIndex = tabIndex

        guard let itemIndex = slotMap.itemIndex(forTabIndex: tabIndex) else {
            return
        }
        barModel.selectedItemIndex = itemIndex
    }

    func setLabels(visible: Bool) {
        guard visible != showsLabels else {
            return
        }
        showsLabels = visible
        rebuildItems()
    }

    func setSPATabs(_ chips: [TabBarShelfChip], selected: UUID?) {
        if spaTabCount != chips.count {
            spaTabCount = chips.count
            rebuildItems()
        }
        barModel.shelfChips = chips
        barModel.mountedChipId = selected
        updateActiveActionIndex()
        // The last product closing empties the grid, so the shelf it opened from closes too.
        guard chips.isEmpty, panelController.open?.action == .spaTabs else {
            return
        }
        setPanel(nil)
    }

    func setChainStatus(_ rows: [ChainConnectionStatusViewModel]) {
        barModel.chainStatusRows = rows
    }

    func setPanel(_ kind: TabBarPanelKind?) {
        panelController.setPanel(kind)
    }

    /// Fades the bar row out or back in. An open centre panel stays. `playsFeedback` false skips the medium impact.
    func setBarHidden(_ hidden: Bool, playsFeedback: Bool = true) {
        barModel.playsBarHiddenFeedback = playsFeedback
        barModel.isBarHidden = hidden
    }

    /// Re-declares the centre content's height. Call it when the content's own
    /// height changes, so an open panel regrows.
    func refreshCenterContentHeight() {
        guard let contentHeightForCenterPanel else {
            return
        }
        let rise = max(0, barModel.availablePanelHeight - Self.centerPanelTopGap)
        let cap = DSTabBarStyle.centerContentHeight(fittingRise: rise)
        let contentHeight: CGFloat =
            switch contentHeightForCenterPanel() {
            case let .fits(declared):
                min(declared, cap)
            case .fill:
                cap
            }
        guard contentHeight != barModel.centerContentHeight else {
            return
        }
        guard hasPerformedInitialLayout else {
            barModel.setCenterContentHeight(contentHeight, animated: false)
            applyBarLiveHeight()
            return
        }
        DSTabBarStyle.animateCenterChange(in: view) {
            self.barModel.setCenterContentHeight(contentHeight)
            self.applyBarLiveHeight()
        }
    }

    func apply(_ context: TabBarChromeContext) {
        foldController.update(context: context)
        foldController.refresh()
        tipController.setBarShown(foldController.state == .shown)
    }

    /// Resolves the visibility state a context would settle on, without committing it — used to
    /// drive the offset alongside an interactive navigation transition.
    func resolvedState(for context: TabBarChromeContext) -> TabBarVisibilityState {
        foldController.resolvedState(for: context)
    }

    /// Moves the chrome toward `state` in sync with an interactive transition. Unlike `apply(state:)`
    /// it runs no animator of its own, so the caller's transition coordinator owns the timing.
    func setInteractiveTarget(_ state: TabBarVisibilityState) {
        foldController.setInteractiveTarget(state)
    }
}

// MARK: - Bar items and panel content

extension TabBarBottomChromeController {
    /// The item list is derived from the slots rather than stored, and the map every index
    /// conversion goes through is rebuilt with it.
    private func rebuildItems() {
        slotMap = TabBarSlotMap(slots: slots)

        barModel.items = slots.map { slot in
            slot.makeBarItem(spaTabCount: spaTabCount, showsLabel: showsLabels, attentionTabs: attentionTabs)
        }
        barModel.appsItemIndex = slotMap.itemIndex(for: .spaTabs)

        setSelectedIndex(selectedTabIndex)
        updateActiveActionIndex()
    }

    /// The Apps item reads active while a panel opened from it is open, and while a product is mounted.
    private func updateActiveActionIndex() {
        let activeAction: TabBarAction? =
            panelController.open?.action ?? (barModel.mountedChipId != nil ? .spaTabs : nil)
        barModel.activeActionItemIndex = activeAction.flatMap { slotMap.itemIndex(for: $0) }
    }

    /// Any committed panel change moves the centre control and the shelf to match, so a close from anywhere
    /// (outside tap, a slot, a fold) also closes whichever surface was showing. The surface that animates the
    /// close reports it back, which releases a queued reopen.
    private func applyCommittedPanel(_ kind: TabBarPanelKind?) {
        // `committedPanel` still names the panel being replaced, so it names the surface a close belongs to.
        let closingSurface = kind == nil ? committedPanel : nil
        let reportClose: () -> Void = { [weak self] in self?.panelController.panelCloseDidComplete() }

        if committedPanel == .content(.scan), kind != .content(.scan) {
            onCenterPanelWillClose?()
            centerPanelContent = nil
        }
        committedPanel = kind

        // The surface whose close reports back is updated last. A synchronous completion can open a queued panel,
        // and the other surface's update must not run after that open and undo it.
        if closingSurface == .shelf {
            barModel.setCenterPanelOpen(kind == .content(.scan))
            barModel.setShelfOpen(kind == .shelf, completion: reportClose)
        } else {
            barModel.setShelfOpen(kind == .shelf)
            barModel.setCenterPanelOpen(
                kind == .content(.scan),
                completion: closingSurface == .content(.scan) ? reportClose : nil
            )
        }

        // A close no surface animates still reports back, so a queued reopen is not stranded.
        if let closingSurface, closingSurface != .content(.scan), closingSurface != .shelf {
            reportClose()
        }
    }

    /// Animates the centre control closed with the requested duration, then commits the close. The progress moves
    /// here rather than in `applyCommittedPanel`, because a commit of an already closed panel does nothing, and a
    /// centre control that was grabbed mid-close would otherwise stay where it was left.
    /// The panel is told first, because the control can remove the content as soon as the progress reaches zero.
    func closeCenterPanel(duration: TimeInterval?) {
        onCenterPanelWillClose?()
        barModel.setCenterPanelOpen(false, duration: duration)
        setPanel(nil)
    }

    /// Commits the panel open before the host focuses search, so the keyboard lift applies as the keyboard rises.
    private func centerPanelSwipeOpenLanding() {
        setPanel(.content(.scan))
        onCenterPanelSwipeOpenLanding?()
    }

    /// Only the centre control's own panel is driven here; another open panel is left alone.
    func centerPanelOpenChanged(_ isOpen: Bool) {
        if isOpen {
            tipController.retireForUserInteraction()
        }
        if isOpen, panelController.open != .content(.scan) {
            setPanel(.content(.scan))
        } else if !isOpen, panelController.open == .content(.scan) {
            setPanel(nil)
        }
        onCenterPanelDidChangeOpen?(isOpen)
    }

    /// The bar's host is a fixed size, so this sets the region of it that accepts touches: the bar row plus
    /// the taller of the resting centre button's stand and the open panel's rise. The open panel is one value,
    /// so the centre panel and the shelf never both contribute a rise.
    func applyBarLiveHeight() {
        let rise: CGFloat =
            switch panelController.open {
            case .content(.scan):
                DSTabBarStyle.centerPanelRise(contentHeight: barModel.centerContentHeight)
            case .shelf:
                min(barModel.shelfContentHeight, barModel.availablePanelHeight)
            case nil,
                 .content:
                0
            }
        chromeSurface.setBarLiveHeight(DSTabBarStyle.height + max(DSTabBarStyle.centerButtonRestingRise, rise))
    }

    /// The room above the bar row down to the top safe area, which the chrome's own layout already measures,
    /// less whatever the keyboard lifts the chrome by. It caps both the shelf's growth and the centre panel's height.
    func refreshAvailablePanelHeight() {
        let availableHeight = max(0, chromeSurface.restingPanelHeight - foldController.keyboardLift)
        guard availableHeight != barModel.availablePanelHeight else {
            return
        }
        barModel.availablePanelHeight = availableHeight
        applyBarLiveHeight()
    }

    func handleSlotTap(itemIndex: Int) {
        tipController.retireForUserInteraction()

        guard !foldController.isFolded else {
            foldController.setUserOverride(.shown, velocityX: 0)
            return
        }

        if let action = slotMap.action(forItemIndex: itemIndex) {
            handleAction(action)
            return
        }

        guard let tabIndex = slotMap.tabIndex(forItemIndex: itemIndex) else {
            return
        }
        let isReselection = tabIndex == selectedTabIndex
        setSelectedIndex(tabIndex)
        onSelect?(tabIndex, isReselection)
    }

    private func handleAction(_ action: TabBarAction) {
        switch action {
        case .connectionStatus:
            #if FEATURE_PRODUCTS
                onConnectionStatusTapped?()
            #else
                panelController.togglePanel(.shelf)
            #endif
        case .spaTabs:
            panelController.togglePanel(.shelf)
        case .scan:
            panelController.togglePanel(.content(.scan))
        }
    }
}
