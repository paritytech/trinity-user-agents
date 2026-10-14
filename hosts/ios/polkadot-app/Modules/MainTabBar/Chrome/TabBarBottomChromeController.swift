import UIKit
import DesignSystem
import PolkadotUI
import SnapKit

final class TabBarBottomChromeController: UIViewController {
    private let chromeSurface = TabBarChromeSurfaceView()
    private let barView = DSTabBarView()
    private let backdropView = DSTabBarBackdropView()

    private weak var contentSafeAreaAdjustedViewController: UIViewController?

    private weak var appliedTabController: UIViewController?
    private weak var appliedContentController: UIViewController?

    private var slots: [TabBarSlot] = []
    private var slotMap = TabBarSlotMap(slots: [])
    private var spaTabCount = 0
    private var badges: [Int: DSTabBarItem.Badge] = [:]
    private var selectedTabIndex = 0
    private var showsLabels = false
    private weak var hostedPanelController: UIViewController?

    private lazy var foldController = TabBarFoldController(
        barView: barView,
        foldSurface: chromeSurface,
        chromeBounds: { [unowned self] in view.bounds },
        grabZoneSink: { [weak self] zone in
            (self?.viewIfLoaded as? TabBarChromePassthroughView)?.foldGrabZone = zone
        },
        closePanel: { [weak self] in
            self?.setPanel(nil, animated: false)
        },
        stateSink: { [weak self] state in
            self?.tipController.setBarShown(state == .shown)
        }
    )

    private lazy var tipController = TabBarTipController(
        host: self,
        barView: barView,
        sequence: TabBarTipOrderedSequence(steps: TabBarTips.steps),
        itemIndex: { [weak self] slot in self?.slotMap.itemIndex(for: slot) },
        statusStripAnchor: { [weak self] in self?.statusStripAnchorProvider?() }
    )

    private lazy var panelController = TabBarPanelController(
        surface: chromeSurface,
        onPanelChanged: { [weak self] kind in
            self?.onPanelChanged?(kind)
        },
        onOpenPanelChanged: { [weak self] kind in
            self?.updateActiveActionIndex()
            (self?.viewIfLoaded as? TabBarChromePassthroughView)?.isOutsideTapEnabled = kind != nil
        },
        dismissTips: { [weak self] in
            self?.tipController.dismissForPanel()
        },
        tearDownContent: { [weak self] in
            self?.detachHostedController()
            self?.chromeSurface.setContentHostedView(nil)
            self?.chromeSurface.setContentConfiguration(nil)
        },
        setBackdropOpen: { [weak self] isOpen, animator in
            self?.backdropView.setOpen(isOpen, animator: animator)
        }
    )

    private lazy var widgetController = TabBarWidgetController { [weak self] in
        self?.updateLayout()
    }

    #if FEATURE_INPUT
        private lazy var inputFocusController = TabBarInputFocusController(
            surface: chromeSurface,
            panelController: panelController,
            content: { [weak self] in self?.hostedPanelController as? TabBarKeyboardTrackingContent }
        )
    #endif

    private lazy var panelDragController = TabBarPanelDragController(
        surface: chromeSurface,
        panelController: panelController,
        dismissSearch: { [weak self] in self?.onPanelDragCommitted?() }
    )

    var onSelect: ((_ index: Int, _ isReselection: Bool) -> Void)?
    var onChipTapped: ((UUID) -> Void)?
    var onChipCloseRequested: ((UUID) -> Void)?
    var onPanelChanged: ((TabBarPanelKind?) -> Void)?
    var onPanelDragCommitted: (() -> Void)?

    /// The chain-status strip is installed by `MainTabBarViewController`, not by the chrome,
    /// so its tip anchor is handed down rather than reached for.
    var statusStripAnchorProvider: (() -> (any UIPopoverPresentationControllerSourceItem)?)?

    private var occupiedHeight: CGFloat {
        guard TabBarVisibilityPolicy.contributesClearance(isTabRoot: foldController.isTabRoot) else {
            return view.safeAreaInsets.bottom
        }
        return DSTabBarView.preferredHeight()
    }

    private var contentClearance: CGFloat {
        max(0, occupiedHeight - view.safeAreaInsets.bottom)
    }

    override func loadView() {
        view = TabBarChromePassthroughView()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        view.backgroundColor = .clear

        installChromeSurface()
        installBar()
        installBackdrop()
        widgetController.install(in: view, below: chromeSurface)

        installOutsideTapRecognizer()

        #if FEATURE_INPUT
            // Observers register on creation.
            _ = inputFocusController
        #endif
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        tipController.start()
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        tipController.stop()
    }

    override func viewSafeAreaInsetsDidChange() {
        super.viewSafeAreaInsetsDidChange()

        updateLayout()
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()

        panelController.refreshHeightAfterLayout()

        foldController.reapplyForWidthChange()
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
        barView.selectedIndex = itemIndex
    }

    func setBadge(_ badge: DSTabBarItem.Badge?, at tabIndex: Int) {
        badges[tabIndex] = badge

        guard let itemIndex = slotMap.itemIndex(forTabIndex: tabIndex) else {
            return
        }
        barView.setBadge(badge, at: itemIndex)
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
        chromeSurface.setChips(chips, selected: selected, closeActionTitle: String(localized: .Common.close))

        if chips.isEmpty || chromeSurface.availablePanelHeight <= 0 {
            if panelController.open == .spaTabs {
                panelController.setPanel(nil, animated: true)
            }
            return
        }

        panelController.refreshHeightAfterChipsChange()
    }

    func setPanel(_ kind: TabBarPanelKind?, animated: Bool) {
        panelController.setPanel(kind, animated: animated)
    }

    func panelDragChanged(translation: CGFloat) {
        panelDragController.dragChanged(translation: translation)
    }

    func panelDragEnded(translation: CGFloat) {
        panelDragController.dragEnded(translation: translation)
    }

    /// Re-measures the open content panel after its hosted controller changed its own size.
    /// The scan panel's camera resizes independently of the keyboard notifications, so the
    /// height cannot be refreshed from those alone.
    func resizeContentPanel() {
        panelController.resizeForContentPanel()
    }

    /// Selecting a different action closes the open panel before opening the new one, so the
    /// change reads as a close followed by an open instead of a silent content swap.
    private func togglePanel(_ kind: TabBarPanelKind) {
        panelController.togglePanel(kind)
    }

    func setContentPanel(_ configuration: (any HashableContentConfiguration)?, for action: TabBarAction) {
        guard panelController.open == .content(action) else { return }

        detachHostedController()
        chromeSurface.setContentConfiguration(configuration)
        panelController.resizeForContentPanel()
    }

    /// A camera controller needs its appearance callbacks, so it is hosted as a child rather than
    /// wrapped in a content view.
    func setContentController(_ controller: UIViewController?, for action: TabBarAction) {
        guard panelController.open == .content(action) else {
            return
        }

        detachHostedController()

        if let controller {
            addChild(controller)
            chromeSurface.setContentHostedView(controller.view)
            controller.didMove(toParent: self)
            hostedPanelController = controller
        } else {
            chromeSurface.setContentHostedView(nil)
        }

        panelController.resizeForContentPanel()
    }

    func apply(
        _ context: TabBarChromeContext,
        animatingAlongside transitionCoordinator: UIViewControllerTransitionCoordinator? = nil
    ) {
        foldController.update(context: context)

        applyLayout(context, animatingAlongside: transitionCoordinator)
        foldController.refresh()
        tipController.setBarShown(foldController.state == .shown)
    }

    func applyLayout(
        _ context: TabBarChromeContext,
        animatingAlongside transitionCoordinator: UIViewControllerTransitionCoordinator? = nil
    ) {
        appliedTabController = context.tabController
        appliedContentController = context.contentController

        updateLayout(animatingAlongside: transitionCoordinator)
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

    func attachWidget(_ configuration: any HashableContentConfiguration, for id: AppWidgetID) {
        widgetController.attach(configuration, for: id)
    }

    func detachWidget(for id: AppWidgetID) {
        widgetController.detach(for: id)
    }
}

// MARK: - Bar items and panel content

private extension TabBarBottomChromeController {
    /// The SPA-tabs action is only shown while there are open apps, so the item list is derived
    /// rather than stored — and with it the map every index conversion goes through.
    func rebuildItems() {
        let effectiveSlots = spaTabCount > 0 ? slots : slots.filter { $0 != .action(.spaTabs) }
        slotMap = TabBarSlotMap(slots: effectiveSlots)

        barView.items = effectiveSlots.enumerated().map { itemIndex, slot in
            let badge = slotMap.tabIndex(forItemIndex: itemIndex).flatMap { badges[$0] }
            return slot.makeBarItem(badge: badge, spaTabCount: spaTabCount, showsLabel: showsLabels)
        }

        setSelectedIndex(selectedTabIndex)
        updateActiveActionIndex()
        tipController.setBarLaidOut(false)
    }

    func updateActiveActionIndex() {
        barView.activeActionIndex = panelController.open.flatMap { slotMap.itemIndex(for: $0.action) }
    }

    func detachHostedController() {
        guard let controller = hostedPanelController else { return }

        controller.willMove(toParent: nil)
        controller.view.removeFromSuperview()
        controller.removeFromParent()
        hostedPanelController = nil
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

        chromeSurface.onChipTapped = { [weak self] id in self?.onChipTapped?(id) }
        chromeSurface.onChipCloseRequested = { [weak self] id in self?.onChipCloseRequested?(id) }
    }

    func installOutsideTapRecognizer() {
        let recognizer = UITapGestureRecognizer(target: self, action: #selector(handleOutsideTap))
        recognizer.delegate = self
        view.addGestureRecognizer(recognizer)
    }

    @objc func handleOutsideTap() {
        guard !barView.isFolded else {
            foldController.setUserOverride(.shown, velocityX: 0)
            return
        }
        panelController.setPanel(nil, animated: true)
    }

    func installBar() {
        chromeSurface.addBar(barView)

        barView.onFoldChangeRequested = { [weak self] folded, velocityX in
            self?.foldController.setUserOverride(folded ? .folded : .shown, velocityX: velocityX)
        }

        barView.onSelect = { [weak self] itemIndex, isReselection in
            self?.tipController.retireForUserInteraction()
            guard let self, let tabIndex = slotMap.tabIndex(forItemIndex: itemIndex) else {
                return
            }
            // The bar moves its own lens on a tap, so the selection has to be recorded here too:
            // `rebuildItems` reapplies `selectedTabIndex`, and a stale one snaps the lens back.
            selectedTabIndex = tabIndex
            onSelect?(tabIndex, isReselection)
        }

        barView.onActionTapped = { [weak self] itemIndex in
            self?.tipController.retireForUserInteraction()
            guard let self, let action = slotMap.action(forItemIndex: itemIndex) else {
                return
            }
            togglePanel(action == .spaTabs ? .spaTabs : .content(action))
        }

        barView.onItemsLaidOut = { [weak self] in
            self?.tipController.setBarLaidOut(true)
        }
    }

    /// Inserted at the bottom so both the tab content and the floating widgets sit behind it.
    func installBackdrop() {
        view.insertSubview(backdropView, at: 0)

        backdropView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }

    func updateLayout(animatingAlongside transitionCoordinator: UIViewControllerTransitionCoordinator? = nil) {
        guard view.window != nil else {
            return
        }

        widgetController.setBottomOffset(-occupiedHeight)
        updateContentSafeAreaInset()

        guard widgetController.hasWidgets else {
            return
        }

        animateFloatingWidgetConstraintChange(with: transitionCoordinator)
    }

    func updateContentSafeAreaInset() {
        let contentController = appliedContentController

        if contentSafeAreaAdjustedViewController !== contentController {
            contentSafeAreaAdjustedViewController?.additionalSafeAreaInsets.bottom = 0
        }

        let tabController = appliedTabController
        let barInset = contentClearance

        guard let contentController else {
            tabController?.additionalSafeAreaInsets.bottom = barInset
            contentSafeAreaAdjustedViewController = nil
            return
        }

        let widgetInset = widgetController.contentHeight(fittingWidth: view.bounds.width)

        if contentController === tabController {
            contentController.additionalSafeAreaInsets.bottom = barInset + widgetInset
        } else {
            tabController?.additionalSafeAreaInsets.bottom = barInset
            contentController.additionalSafeAreaInsets.bottom = widgetInset
        }

        contentSafeAreaAdjustedViewController = widgetInset > 0 ? contentController : nil
    }

    func animateFloatingWidgetConstraintChange(with transitionCoordinator: UIViewControllerTransitionCoordinator?) {
        view.setNeedsLayout()

        guard let transitionCoordinator else {
            view.layoutIfNeeded()
            return
        }

        transitionCoordinator.animate(
            alongsideTransition: { [weak self] _ in
                self?.view.layoutIfNeeded()
            }
        )
    }
}

// MARK: - UIGestureRecognizerDelegate

extension TabBarBottomChromeController: UIGestureRecognizerDelegate {
    func gestureRecognizer(_: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        touch.view === view
    }
}
