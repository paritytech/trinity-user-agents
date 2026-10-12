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

    // Extensions cannot hold stored properties, so the widget layout
    // in MainTabBarViewController+Widgets.swift reads these from here.
    weak var contentSafeAreaAdjustedViewController: UIViewController?
    weak var appliedTabController: UIViewController?
    weak var appliedContentController: UIViewController?

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

    #if FEATURE_INPUT
        /// The live scan panel, read for its declared height. Nil while the centre control has no content.
        private weak var scanPanel: ScanPanelViewController?
    #endif

    private weak var scanPanelController: (UIViewController & ScanPanelScannerControlling)?

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
        #if FEATURE_INPUT
            chromeController.contentHeightForCenterPanel = { [weak self] width in
                self?.scanPanel?.declaredContentHeight(forPanelWidth: width)
                    ?? .fits(
                        ScanPanelContentHeight.height(
                            forPanelWidth: width,
                            searchRowHeight: ScanPanelContentHeight.embeddedSearchRowHeight
                        )
                    )
            }
            chromeController.onCenterPanelWillClose = { [weak self] in self?.scanPanel?.cancelSearch() }
        #else
            chromeController.contentHeightForCenterPanel = { width in
                .fits(
                    ScanPanelContentHeight.height(
                        forPanelWidth: width,
                        searchRowHeight: ScanPanelContentHeight.plainSearchRowHeight
                    )
                )
            }
        #endif

        chromeController.onCenterPanelDidChangeOpen = { [weak self] isOpen in
            self?.scanPanelController?.setCaptureActive(isOpen)
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

    func chromeContext(
        tabController: UIViewController,
        stack: [UIViewController],
        showing target: UIViewController
    ) -> TabBarChromeContext {
        TabBarChromeContext(
            tabController: tabController,
            contentController: target,
            isTabRoot: TabBarHiddenPolicy.isTabRoot(in: stack, showing: target),
            foldDerived: TabBarHiddenPolicy.deriveFoldState(in: stack, showing: target),
            screen: target
        )
    }

    func stayingChromeContext(
        in navigationController: UINavigationController,
        context: UIViewControllerTransitionCoordinatorContext
    ) -> TabBarChromeContext? {
        guard let staying = context.viewController(forKey: .from) else {
            return nil
        }
        let restored = TabBarHiddenPolicy.stackAfterCancelledPop(
            stack: navigationController.viewControllers,
            staying: staying
        )
        return chromeContext(tabController: navigationController, stack: restored, showing: staying)
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

    func track(
        of navigationController: AppNavigationController,
        showing target: UIViewController,
        animated: Bool
    ) {
        let incoming = chromeContext(
            tabController: navigationController,
            stack: navigationController.viewControllers,
            showing: target
        )

        guard
            animated,
            let coordinator = navigationController.transitionCoordinator,
            coordinator.isInteractive
        else {
            applyChrome(
                incoming,
                animatingAlongside: animated ? navigationController.transitionCoordinator : nil
            )
            return
        }

        applyWidgetLayout(for: incoming, animatingAlongside: coordinator)

        let targetState = chromeController.resolvedState(for: incoming)
        coordinator.animate(alongsideTransition: { [weak self] _ in
            self?.chromeController.setInteractiveTarget(targetState)
        })

        coordinator.notifyWhenInteractionChanges { [weak self] context in
            guard let self else {
                return
            }
            let staying = context.isCancelled
                ? stayingChromeContext(in: navigationController, context: context)
                : nil
            applyChrome(staying ?? incoming)
        }
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

    func makeSettingsNavigation() -> UINavigationController? {
        viewFactory.makeSettingsNavigation()
    }
}

// MARK: - Scan panel

extension MainTabBarViewController {
    func openScanPanel() {
        chromeController.setPanel(.content(.scan))
    }

    func makeScanPanel() -> UIViewController? {
        let controller = viewFactory.makeScanController()
        scanPanelController = controller
        controller?.setCaptureActive(false)

        #if FEATURE_INPUT
            // Opening the chat selects its tab, and tab selection already closes the panel, so this
            // path does not close it again.
            scanPanel = controller
            controller?.onChatFound = { [weak self] model in
                self?.presenter.didFindChat(model)
            }
            controller?.onContentHeightChanged = { [weak self] in
                self?.chromeController.refreshCenterContentHeight()
            }
        #else
            controller?.onSearchTap = { [weak self] in
                self?.chromeController.setPanel(nil)
                self?.presenter.didRequestContactSearch()
            }
        #endif
        return controller
    }
}

// MARK: - AppNavigationControllerTransitionObserving

extension MainTabBarViewController: AppNavigationControllerTransitionObserving {
    func appNavigationController(
        _ navigationController: AppNavigationController,
        willShow viewController: UIViewController,
        animated: Bool
    ) {
        track(of: navigationController, showing: viewController, animated: animated)
    }

    func appNavigationController(
        _ navigationController: AppNavigationController,
        didShow viewController: UIViewController,
        animated _: Bool
    ) {
        applyChrome(chromeContext(
            tabController: navigationController,
            stack: navigationController.viewControllers,
            showing: viewController
        ))
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
