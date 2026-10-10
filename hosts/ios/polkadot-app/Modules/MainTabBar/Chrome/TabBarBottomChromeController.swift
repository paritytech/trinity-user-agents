import UIKit
import SwiftUI
import DesignSystem
import PolkadotUI
import SnapKit

final class TabBarBottomChromeController: UIViewController {
    private let chromeSurface = TabBarChromeSurfaceView()
    private let barModel = TabBarBarViewModel()
    private lazy var barHost = UIHostingController(rootView: TabBarBarView(model: barModel))
    private let backdropView = DSTabBarBackdropView()

    private var slots: [TabBarSlot] = []
    private var slotMap = TabBarSlotMap(slots: [])
    private var spaTabCount = 0
    private var selectedTabIndex = 0
    private var showsLabels = false
    /// The first settled layout places the bar. Animating it would spring the bar in from its unplaced frame at launch.
    private var hasPerformedInitialLayout = false

    private lazy var foldController = TabBarFoldController(
        foldSurface: chromeSurface,
        chromeBounds: { [unowned self] in view.bounds },
        grabZoneSink: { [weak self] zone in
            (self?.viewIfLoaded as? TabBarChromePassthroughView)?.foldGrabZone = zone
        },
        closePanel: { [weak self] in
            self?.barModel.setCenterPanelOpen(false)
            self?.setPanel(nil, animated: false)
        }
    )

    private lazy var tipController = TabBarTipController(
        host: self,
        sequence: TabBarTipOrderedSequence(steps: TabBarTips.steps),
        statusStripAnchor: { [weak self] in self?.statusStripAnchorProvider?() }
    )

    private lazy var panelController = TabBarPanelController(
        onPanelChanged: { [weak self] kind in
            self?.syncCenterPanel(with: kind)
            self?.syncShelf(with: kind)
        },
        onOpenPanelChanged: { [weak self] kind in
            self?.updateActiveActionIndex()
            self?.applyBarLiveHeight()
            (self?.viewIfLoaded as? TabBarChromePassthroughView)?.isOutsideTapEnabled = kind != nil
        },
        setBackdropOpen: { [weak self] isOpen, animator in
            self?.backdropView.setOpen(isOpen, animator: animator)
        }
    )

    var onSelect: ((_ index: Int, _ isReselection: Bool) -> Void)?
    var onChipTapped: ((UUID) -> Void)?
    var onChipCloseRequested: ((UUID) -> Void)?
    var onConnectionStatusTapped: (() -> Void)?
    var onSettingsTapped: (() -> Void)?

    /// Builds the centre control's content. Called when the control first shows it, not when the panel commits.
    var makeCenterContent: (() -> UIViewController?)?

    /// Declares the centre content's height for a panel of the given width. The control sizes itself from this
    /// before any content exists, so it cannot be measured from the content.
    var contentHeightForCenterPanel: ((CGFloat) -> CGFloat)?

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
        return DSTabBarStyle.restingHeight
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

        installChromeSurface()
        installBar()
        installBackdrop()

        installOutsideTapRecognizer()
        installKeyboardObserver()
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()

        // Order matters: the centre content height is clamped to the available height the first call writes.
        refreshAvailablePanelHeight()
        refreshCenterContentHeight()

        foldController.reapplyForWidthChange()
        if view.window != nil, view.bounds.width > 0, view.bounds.height > 0 {
            hasPerformedInitialLayout = true
        }
    }

    func setItems(_ slots: [TabBarSlot]) {
        self.slots = slots
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

    func setSPATabs(_ chips: [DSTabBarChip], selected: UUID?) {
        if spaTabCount != chips.count {
            spaTabCount = chips.count
            rebuildItems()
        }
        barModel.shelfChips = chips
        barModel.mountedChipId = selected
        updateActiveActionIndex()
    }

    func setPanel(_ kind: TabBarPanelKind?, animated: Bool) {
        panelController.setPanel(kind, animated: animated)
    }

    /// Re-declares the centre content's height at the current bar width. Call it when the content's own
    /// height changes, so an open panel regrows.
    func refreshCenterContentHeight() {
        guard let contentHeightForCenterPanel else {
            return
        }
        let width = DSTabBarStyle.centerPanelWidth(inRowOfWidth: barHost.view.bounds.width)
        let contentHeight = min(
            contentHeightForCenterPanel(width),
            DSTabBarStyle.centerContentHeight(fittingPanelHeight: barModel.availablePanelHeight)
        )
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

    /// Selecting a different action closes the open panel before opening the new one, so the
    /// change reads as a close followed by an open instead of a silent content swap.
    private func togglePanel(_ kind: TabBarPanelKind) {
        panelController.togglePanel(kind)
    }

    func apply(_ context: TabBarChromeContext) {
        foldController.update(context: context)
        foldController.refresh()
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

private extension TabBarBottomChromeController {
    /// The item list is derived from the slots rather than stored, and the map every index
    /// conversion goes through is rebuilt with it.
    func rebuildItems() {
        slotMap = TabBarSlotMap(slots: slots)

        barModel.items = slots.map { slot in
            slot.makeBarItem(spaTabCount: spaTabCount, showsLabel: showsLabels)
        }

        setSelectedIndex(selectedTabIndex)
        updateActiveActionIndex()
    }

    /// The Apps item reads active while a panel opened from it is open, and while a product is mounted.
    func updateActiveActionIndex() {
        let activeAction: TabBarAction? =
            panelController.open?.action ?? (barModel.mountedChipId != nil ? .spaTabs : nil)
        barModel.activeActionItemIndex = activeAction.flatMap { slotMap.itemIndex(for: $0) }
    }

    /// Any committed panel change moves the centre control to match, so a close from anywhere (outside tap,
    /// a slot, a fold) also closes the control.
    func syncCenterPanel(with kind: TabBarPanelKind?) {
        barModel.setCenterPanelOpen(kind == .content(.scan))
    }

    /// The shelf is drawn by the bar row, so it follows the committed panel the way the centre control does.
    func syncShelf(with kind: TabBarPanelKind?) {
        barModel.isShelfOpen = kind == .shelf
    }

    /// Only the centre control's own panel is driven here; another open panel is left alone.
    func centerPanelOpenChanged(_ isOpen: Bool) {
        if isOpen, panelController.open != .content(.scan) {
            setPanel(.content(.scan), animated: true)
        } else if !isOpen, panelController.open == .content(.scan) {
            setPanel(nil, animated: true)
        }
    }

    /// The bar's host is a fixed size, so this sets the region of it that accepts touches: the bar row plus
    /// the open panel's rise. The open panel is one value, so the centre panel and the shelf never both
    /// contribute a rise.
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
        chromeSurface.setBarLiveHeight(DSTabBarStyle.height + rise)
    }

    /// The room above the bar row down to the top safe area, which the chrome's own layout already measures.
    /// It caps both the shelf's growth and the centre panel's height.
    func refreshAvailablePanelHeight() {
        let availableHeight = max(0, chromeSurface.availablePanelHeight)
        guard availableHeight != barModel.availablePanelHeight else {
            return
        }
        barModel.availablePanelHeight = availableHeight
        applyBarLiveHeight()
    }

    func handleSlotTap(itemIndex: Int) {
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

    func handleAction(_ action: TabBarAction) {
        switch action {
        case .connectionStatus:
            onConnectionStatusTapped?()
        case .spaTabs:
            togglePanel(.shelf)
        case .scan:
            togglePanel(.content(.scan))
        }
    }
}

// MARK: - Layout

private extension TabBarBottomChromeController {
    func installChromeSurface() {
        chromeSurface.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(chromeSurface)
        chromeSurface.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }

    func installOutsideTapRecognizer() {
        let recognizer = UITapGestureRecognizer(target: self, action: #selector(handleOutsideTap))
        recognizer.delegate = self
        view.addGestureRecognizer(recognizer)
    }

    @objc func handleOutsideTap() {
        guard !foldController.isFolded else {
            foldController.setUserOverride(.shown, velocityX: 0)
            return
        }
        panelController.setPanel(nil, animated: true)
    }

    func installKeyboardObserver() {
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleKeyboardFrameChange(_:)),
            name: UIResponder.keyboardWillChangeFrameNotification,
            object: nil
        )
    }

    /// While the scan panel is open, the chrome moves up so the search row stays above the keyboard, keeping
    /// the bar's resting gap. Any other keyboard, or a closed panel, returns the chrome to rest.
    @objc func handleKeyboardFrameChange(_ notification: Notification) {
        guard let keyboardFrame = notification.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? CGRect else {
            return
        }
        let keyboardTop = view.convert(keyboardFrame, from: nil).minY
        let isScanPanelOpen = panelController.open == .content(.scan)
        let lift = isScanPanelOpen ? max(0, view.bounds.maxY - keyboardTop) : 0
        DSTabBarStyle.animateCenterChange(in: view) {
            self.foldController.setKeyboardLift(lift)
        }
    }

    @objc func handleFoldPan(_ recognizer: UIPanGestureRecognizer) {
        guard recognizer.state == .ended else {
            return
        }

        foldController.foldPanEnded(
            translationX: recognizer.translation(in: chromeSurface).x,
            velocityX: recognizer.velocity(in: chromeSurface).x,
            barWidth: barHost.view.bounds.width
        )
    }

    func installBar() {
        addChild(barHost)
        chromeSurface.addBar(barHost.view)
        barHost.didMove(toParent: self)
        barHost.view.backgroundColor = .clear
        // The pinned bar geometry owns the home-indicator gap. The default safe area would apply it a second time.
        barHost.safeAreaRegions = []

        barModel.onSlotTapped = { [weak self] itemIndex in
            self?.handleSlotTap(itemIndex: itemIndex)
        }
        barModel.makeCenterContent = { [weak self] in
            self?.makeCenterContent?()
        }
        barModel.onCenterPanelOpenChanged = { [weak self] isOpen in
            self?.centerPanelOpenChanged(isOpen)
        }
        barModel.onShelfContentHeightChanged = { [weak self] in
            self?.applyBarLiveHeight()
        }
        barModel.onShelfChipTapped = { [weak self] id in
            self?.onChipTapped?(id)
        }
        barModel.onShelfChipCloseRequested = { [weak self] id in
            self?.onChipCloseRequested?(id)
        }
        barModel.onShelfSettingsTapped = { [weak self] in
            self?.onSettingsTapped?()
        }
        barModel.onShelfConnectionStatusTapped = { [weak self] in
            self?.onConnectionStatusTapped?()
        }

        let foldRecognizer = UIPanGestureRecognizer(target: self, action: #selector(handleFoldPan(_:)))
        foldRecognizer.delegate = self
        chromeSurface.addGestureRecognizer(foldRecognizer)
    }

    /// Sits below the chrome surface so the dim covers the content and the floating widgets,
    /// which sit below the chrome view.
    func installBackdrop() {
        view.insertSubview(backdropView, at: 0)

        backdropView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }

    /// A fold drag starts on the bar row, outside the centre hole where the centre control sits.
    func canStartFoldDrag(at touch: UITouch) -> Bool {
        // The hosting view is taller than the row while the centre panel is open, and the row sits at its bottom.
        let bounds = barHost.view.bounds
        let row = CGRect(
            x: 0,
            y: bounds.height - DSTabBarStyle.height,
            width: bounds.width,
            height: DSTabBarStyle.height
        )
        let point = touch.location(in: barHost.view)
        guard row.contains(point) else {
            return false
        }
        let pointInRow = CGPoint(x: point.x, y: point.y - row.minY)
        return !DSTabBarStyle.isInCenterHole(pointInRow, inRowOfWidth: row.width)
    }
}

// MARK: - UIGestureRecognizerDelegate

extension TabBarBottomChromeController: UIGestureRecognizerDelegate {
    func gestureRecognizer(_ recognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        guard recognizer is UIPanGestureRecognizer else {
            return touch.view === view
        }
        return canStartFoldDrag(at: touch)
    }

    /// Only a drag that leads horizontally folds the bar, so a vertical scroll past it is left alone.
    func gestureRecognizerShouldBegin(_ recognizer: UIGestureRecognizer) -> Bool {
        guard let pan = recognizer as? UIPanGestureRecognizer else {
            return true
        }
        let translation = pan.translation(in: chromeSurface)
        return abs(translation.x) > abs(translation.y)
    }
}
