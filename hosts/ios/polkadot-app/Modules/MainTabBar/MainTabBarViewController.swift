import UIKit
import SwiftUI
import PolkadotUI
import SnapKit
import DesignSystem
import UIKitExt

final class MainTabBarViewController: UIViewController {
    let presenter: MainTabBarPresenterProtocol
    let viewFactory: TabFactoryProtocol
    let browserCoordinator: SPABrowserCoordinating
    let flowStateProvider: any SPAFlowStateProviding

    let chromeController = TabBarBottomChromeController()

    lazy var widgetController = TabBarWidgetController { [weak self] in
        self?.updateWidgetLayout()
    }

    private lazy var statusBarHost = UIHostingController(rootView: ChainConnectionStatusBarView(models: []))

    /// The rings sit at the trailing end of the full-width strip, so the tip anchors here
    /// rather than at the host view, whose centre is empty. Inset from the trailing edge so the
    /// popover is not clamped against the screen edge.
    private let chainStatusAnchorGuide = UILayoutGuide()

    private static let chainStatusAnchorInset: CGFloat = 26

    private var chainStatusAnchorWidth: Constraint?

    lazy var container = TabBarContainer(hostController: self)

    private var tabs: [TabBarItem] = []
    private var controllerByItem: [TabBarItem: UIViewController] = [:]
    var spaChipViewModels: [SPATabChipViewModel] = []
    var productGamePill: ProductGamePillViewModel?

    /// The live scan panel. Nil while the centre control has no content.
    private weak var scanPanel: ScanPanelViewController?

    /// Set while a swipe on the centre button carries the panel toward Search, before it lands.
    private var isSearchSwipePending = false
    /// Set while the landing focuses search, so the bar's hide plays no feedback.
    private var isFocusingFromSwipe = false

    init(
        presenter: MainTabBarPresenterProtocol,
        viewFactory: TabFactoryProtocol,
        browserCoordinator: SPABrowserCoordinating,
        flowStateProvider: any SPAFlowStateProviding
    ) {
        self.presenter = presenter
        self.viewFactory = viewFactory
        self.browserCoordinator = browserCoordinator
        self.flowStateProvider = flowStateProvider
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        view.backgroundColor = .bgSurfaceMain

        installStatusBar()

        installChromeController()
        installWidgets()

        chromeController.statusStripAnchorProvider = { [weak self] in self?.chainStatusAnchorGuide }
        chromeController.makeCenterContent = { [weak self] in self?.makeScanPanel() }
        chromeController.contentHeightForCenterPanel = { [weak self] in
            guard let self else {
                return .fits(ScanPanelViewLayout.declaredHeight)
            }
            return isSearchSwipePending
                ? .fill
                : scanPanel?.declaredContentHeight() ?? .fits(ScanPanelViewLayout.declaredHeight)
        }
        chromeController.onCenterPanelWillClose = { [weak self] in
            self?.cancelSearchSwipe()
            self?.scanPanel?.prepareForClose()
        }
        chromeController.onCenterPanelSwipeOpenBegan = { [weak self] in self?.beginSearchSwipe() }
        chromeController.onCenterPanelSwipeOpenLanding = { [weak self] in self?.landSearchSwipe() }

        // Close does not stop capture: the panel's teardown releases the camera, and an explicit stop
        // here can race the preview layer's session attach.
        chromeController.onCenterPanelDidChangeOpen = { [weak self] isOpen in
            if !isOpen {
                self?.cancelSearchSwipe()
            }
            guard isOpen else { return }
            self?.scanPanel?.setCaptureActive(true)
        }

        chromeController.onSelect = { [weak self] index, isReselection in
            self?.handleSelection(index: index, isReselection: isReselection)
        }

        chromeController.onConnectionStatusTapped = { [weak self] in
            self?.chromeController.setPanel(nil)
            self?.presenter.didRequestConnectionStatus()
        }

        chromeController.onSettingsTapped = { [weak self] in
            self?.chromeController.setPanel(nil)
            self?.presenter.didRequestSettings()
        }

        chromeController.onChipTapped = { [weak self] id in
            _ = self?.mountExistingTab { $0.id == id }
        }

        chromeController.onChipCloseRequested = { [weak self] id in
            self?.closeSPA(tabId: id)
        }

        container.onContentInsetInvalidated = { [weak self] in
            guard let self else {
                return
            }
            applyWidgetLayout(for: chromeContext(for: container.selectedController))
        }

        chromeController.onBottomClearanceInvalidated = { [weak self] in
            guard let self else {
                return
            }
            applyWidgetLayout(for: chromeContext(for: container.selectedController))
        }

        presenter.setup()
    }

    override func viewSafeAreaInsetsDidChange() {
        super.viewSafeAreaInsetsDidChange()

        applyWidgetLayout(for: chromeContext(for: container.selectedController))
    }

    override var childForStatusBarStyle: UIViewController? {
        container.selectedController
    }

    override var childForHomeIndicatorAutoHidden: UIViewController? {
        container.selectedController
    }

    override var supportedInterfaceOrientations: UIInterfaceOrientationMask {
        container.selectedController?.supportedInterfaceOrientations ?? super.supportedInterfaceOrientations
    }
}

// MARK: - Private

private extension MainTabBarViewController {
    func installStatusBar() {
        additionalSafeAreaInsets.top = ChainConnectionStatusBarView.preferredHeight

        addChild(statusBarHost)
        statusBarHost.view.backgroundColor = .clear
        view.addSubview(statusBarHost.view)
        statusBarHost.view.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview()
            make.bottom.equalTo(view.safeAreaLayoutGuide.snp.top)
        }

        statusBarHost.view.addLayoutGuide(chainStatusAnchorGuide)
        chainStatusAnchorGuide.snp.makeConstraints { make in
            make.trailing.equalTo(statusBarHost.view).offset(-Self.chainStatusAnchorInset)
            make.top.bottom.equalTo(statusBarHost.view)
            chainStatusAnchorWidth = make.width.equalTo(1).constraint
        }

        statusBarHost.didMove(toParent: self)
    }

    func installChromeController() {
        addChild(chromeController)
        view.addSubview(chromeController.view)

        chromeController.view.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
        chromeController.didMove(toParent: self)
    }

    func installWidgets() {
        widgetController.install(in: view, below: chromeController.view)
    }

    func chromeContext(for controller: UIViewController?) -> TabBarChromeContext {
        if case .spa = container.selection, let spa = container.selectedController {
            return .spa(spa)
        }

        guard
            let navigation = controller as? UINavigationController,
            let top = navigation.topViewController
        else {
            return TabBarChromeContext(
                tabController: controller,
                contentController: controller,
                isTabRoot: true,
                foldDerived: .none,
                screen: controller
            )
        }
        return chromeContext(tabController: navigation, stack: navigation.viewControllers, showing: top)
    }

    func handleReselection() {
        switch TabBarReselectionPolicy.action(for: container.selectedController) {
        case let .popToRoot(navigation):
            navigation.popToRootViewController(animated: true)
        case let .scrollToTop(target):
            scrollToTop(in: target)
        case .ignore:
            break
        }
    }

    func scrollToTop(in target: UIViewController) {
        guard
            let root = target.viewIfLoaded,
            let scrollView = TabBarScrollToTopLocator.scrollView(in: root)
        else {
            return
        }

        TabBarScrollToTopLocator.scrollToTop(scrollView)
    }
}

// MARK: - Selection

extension MainTabBarViewController {
    func handleSelection(index: Int, isReselection: Bool) {
        chromeController.setPanel(nil)

        guard tabs.indices.contains(index) else { return }

        guard !isReselection || container.selection.isSPA else {
            handleReselection()
            return
        }

        container.select(index: index)
        refreshChrome()
    }
}

// MARK: - MainTabBarViewProtocol

extension MainTabBarViewController: MainTabBarViewProtocol {
    func show(slots: [TabBarSlot], selecting tab: TabBarItem) {
        let content = slots.compactMap(\.tab).compactMap { item in
            viewFactory.view(for: item).map { (item: item, controller: $0) }
        }
        content.forEach { entry in
            controllerByItem[entry.item] = entry.controller
            (entry.controller as? AppNavigationController)?.transitionObserver = self
        }

        tabs = content.map(\.item)

        let index = tabs.firstIndex(of: tab) ?? 0

        chromeController.setItems(slots)
        chromeController.setSelectedIndex(index)
        container.setControllers(content.map(\.controller), selecting: index)
        refreshChrome()
    }

    func select(tab: TabBarItem) {
        chromeController.setPanel(nil)

        guard let index = tabs.firstIndex(of: tab) else { return }

        chromeController.setSelectedIndex(index)
        container.select(index: index)
        refreshChrome()
    }

    func setLabels(visible: Bool) {
        chromeController.setLabels(visible: visible)
    }

    func view(for tab: TabBarItem) -> UIViewController? {
        controllerByItem[tab]
    }

    func showSPATabs(_ viewModels: [SPATabChipViewModel]) {
        spaChipViewModels = viewModels
        applyChips()
    }

    func showChainStatus(_ models: [ChainConnectionStatusViewModel]) {
        statusBarHost.rootView = ChainConnectionStatusBarView(models: models)
        let width = max(1, ChainConnectionStatusBarView.ringsWidth(count: models.count))
        chainStatusAnchorWidth?.update(offset: width)
    }

    func showProductGamePill(_ viewModel: ProductGamePillViewModel?) {
        productGamePill = viewModel
        applyProductGamePill()
    }
}

// MARK: - Scan panel

extension MainTabBarViewController {
    func openScanPanel() {
        chromeController.setPanel(.content(.scan))
    }

    func makeScanPanel() -> UIViewController? {
        let controller = viewFactory.makeScanController()
        scanPanel = controller
        controller?.setCaptureActive(false)

        // Opening the chat selects its tab, and tab selection already closes the panel, so this
        // path does not close it again.
        controller?.onChatFound = { [weak self] model in
            self?.presenter.didFindChat(model)
        }
        controller?.onContentHeightChanged = { [weak self] in
            self?.chromeController.refreshCenterContentHeight()
        }
        controller?.onSearchFocusChanged = { [weak self] focused in
            guard let self else {
                return
            }
            chromeController.setBarHidden(focused, playsFeedback: !isFocusingFromSwipe)
        }
        if isSearchSwipePending {
            controller?.setSearchPending(true)
        }
        return controller
    }
}

private extension MainTabBarViewController {
    func beginSearchSwipe() {
        isSearchSwipePending = true
        scanPanel?.setSearchPending(true)
        chromeController.refreshCenterContentHeight()
    }

    func landSearchSwipe() {
        isSearchSwipePending = false
        isFocusingFromSwipe = true
        scanPanel?.focusSearch()
        isFocusingFromSwipe = false
    }

    /// Drops a swipe that did not land, and returns the scan panel to the Scan state's layout.
    func cancelSearchSwipe() {
        isSearchSwipePending = false
        scanPanel?.setSearchPending(false)
    }
}

extension MainTabBarViewController: TopmostChildProviding {
    var topmostChild: UIViewController? {
        container.selectedController
    }
}

extension MainTabBarViewController {
    func refreshChrome() {
        applyChrome(chromeContext(for: container.selectedController))
        applyChips()
    }

    func applyChrome(
        _ context: TabBarChromeContext,
        animatingAlongside transitionCoordinator: UIViewControllerTransitionCoordinator? = nil
    ) {
        chromeController.apply(context)
        applyWidgetLayout(for: context, animatingAlongside: transitionCoordinator)
    }

    func attachWidget(_ configuration: any HashableContentConfiguration, for id: AppWidgetID) {
        widgetController.attach(configuration, for: id)
    }

    func detachWidget(for id: AppWidgetID) {
        widgetController.detach(for: id)
    }
}

extension MainTabBarViewController {
    /// The chrome sits over every tab, so a Pocket card flying out of the
    /// Wallet fades it along with the Pocket's own content.
    func setChromeAlpha(_ alpha: CGFloat) {
        statusBarHost.view.alpha = alpha
        chromeController.view.alpha = alpha
    }
}
