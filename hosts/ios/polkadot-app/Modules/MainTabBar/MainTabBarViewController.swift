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

    private lazy var statusBarHost = UIHostingController(rootView: ChainConnectionStatusBarView(models: []))

    /// The rings sit at the trailing end of the full-width strip, so the tip anchors here
    /// rather than at the host view, whose centre is empty. Inset from the trailing edge so the
    /// popover is not clamped against the screen edge.
    private let chainStatusAnchorGuide = UILayoutGuide()

    private static let chainStatusAnchorInset: CGFloat = 26

    private var chainStatusAnchorWidth: Constraint?

    lazy var container = TabBarContainer(hostController: self)

    private var tabs: [TabBarItem] = []
    private var badges: [TabBarItem: TabBarBadge] = [:]
    private var controllerByItem: [TabBarItem: UIViewController] = [:]
    var spaChipViewModels: [SPATabChipViewModel] = []
    var productGamePill: ProductGamePillViewModel?

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

        chromeController.statusStripAnchorProvider = { [weak self] in self?.chainStatusAnchorGuide }

        chromeController.onSelect = { [weak self] index, isReselection in
            self?.handleSelection(index: index, isReselection: isReselection)
        }

        chromeController.onPanelChanged = { [weak self] kind in
            guard let action = kind?.contentAction else {
                return
            }
            self?.presenter.didRequestContentPanel(for: action)
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
            chromeController.applyLayout(chromeContext(for: container.selectedController))
        }

        presenter.setup()
    }

    override func viewSafeAreaInsetsDidChange() {
        super.viewSafeAreaInsetsDidChange()

        chromeController.applyLayout(chromeContext(for: container.selectedController))
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
            chromeController.apply(
                incoming,
                animatingAlongside: animated ? navigationController.transitionCoordinator : nil
            )
            return
        }

        chromeController.applyLayout(incoming, animatingAlongside: coordinator)

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
            chromeController.apply(staying ?? incoming)
        }
    }
}

// MARK: - Selection

extension MainTabBarViewController {
    func handleSelection(index: Int, isReselection: Bool) {
        chromeController.setPanel(nil, animated: true)

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
        badges.forEach { setBadge($0.value, for: $0.key) }
        container.setControllers(content.map(\.controller), selecting: index)
        refreshChrome()
    }

    func select(tab: TabBarItem) {
        chromeController.setPanel(nil, animated: true)

        guard let index = tabs.firstIndex(of: tab) else { return }

        chromeController.setSelectedIndex(index)
        container.select(index: index)
        refreshChrome()
    }

    func setBadge(_ badge: TabBarBadge?, for tab: TabBarItem) {
        badges[tab] = badge
        guard let index = tabs.firstIndex(of: tab) else {
            return
        }
        chromeController.setBadge(badge.map { _ in .attention }, at: index)
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

    func showTabBarPanelContent(_ configuration: (any HashableContentConfiguration)?, for action: TabBarAction) {
        chromeController.setContentPanel(configuration, for: action)
    }

    func showScanPanel() {
        let controller = viewFactory.makeScanController()
        chromeController.setContentController(controller, for: .scan)

        controller?.onPanelDragChanged = { [weak self] translation in
            self?.chromeController.panelDragChanged(translation: translation)
        }

        controller?.onPanelDragEnded = { [weak self] translation in
            self?.chromeController.panelDragEnded(translation: translation)
        }

        #if FEATURE_INPUT
            // Opening the chat selects its tab, and tab selection closes the panel. A second close
            // here would cancel that animation in place and leave the backdrop and panel frozen
            // mid-way.
            controller?.onChatFound = { [weak self] model in
                self?.presenter.didFindChat(model)
            }

            controller?.onContentHeightChanged = { [weak self] in
                self?.chromeController.resizeContentPanel()
            }

            chromeController.onPanelDragCommitted = { [weak controller] in
                controller?.cancelSearch()
            }
        #else
            controller?.onSearchTap = { [weak self] in
                self?.chromeController.setPanel(nil, animated: true)
                self?.presenter.didRequestContactSearch()
            }
        #endif
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
    /// Opens the scan panel from outside the bar, as a tap on the `.scan` action would.
    func openScanPanel() {
        chromeController.setPanel(.content(.scan), animated: true)
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
        chromeController.apply(chromeContext(
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
        chromeController.apply(chromeContext(for: container.selectedController))
        applyChips()
    }

    func attachWidget(_ configuration: any HashableContentConfiguration, for id: AppWidgetID) {
        chromeController.attachWidget(configuration, for: id)
    }

    func detachWidget(for id: AppWidgetID) {
        chromeController.detachWidget(for: id)
    }
}
