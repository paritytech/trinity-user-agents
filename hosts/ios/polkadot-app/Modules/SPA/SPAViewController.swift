import DesignSystem
import FoundationExt
import PolkadotUI
import Products
import func TrUAPIHost.productContentScheme
import UIKit
import UIKitExt
import WebKit

final class SPAViewController: UIViewController, ViewHolder {
    typealias RootViewType = SPAViewLayout

    let presenter: SPAPresenterProtocol
    let configuration: SPAConfiguration
    let hostProvider: ProductHostProviding

    private let schemeHandlerProxy: SchemeHandlerProxy
    private let logger: LoggerProtocol

    private(set) var statusBarTitle: String?
    var onStatusBarTitleChanged: (() -> Void)?
    var onStatusStripThemeChanged: (() -> Void)?

    private var titleObservation: NSKeyValueObservation?
    private var pageBackgroundTone: PageBackgroundTone?
    private var pageBackgroundObservations: [NSKeyValueObservation] = []
    private var didSetupWebViewLayout = false
    private var loadFailure: ErrorContent?
    private var loadingStartedAt: Date?
    private var pendingFailureTask: Task<Void, Never>?
    private lazy var loadIndicator = SPALoadIndicator(
        progressView: rootView.loadProgressView,
        statusLabel: rootView.loadStatusLabel
    )

    init(
        presenter: SPAPresenterProtocol,
        configuration: SPAConfiguration,
        schemeHandlerProxy: SchemeHandlerProxy,
        logger: LoggerProtocol,
        hostProvider: ProductHostProviding
    ) {
        self.presenter = presenter
        self.configuration = configuration
        self.schemeHandlerProxy = schemeHandlerProxy
        self.logger = logger
        self.hostProvider = hostProvider
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        let webViewConfiguration = WKWebViewConfiguration()

        webViewConfiguration.applicationNameForUserAgent = ProductUserAgent.applicationName
        webViewConfiguration.allowsInlineMediaPlayback = true
        webViewConfiguration.mediaTypesRequiringUserActionForPlayback = []

        webViewConfiguration.setURLSchemeHandler(
            schemeHandlerProxy,
            forURLScheme: productContentScheme()
        )
        view = SPAViewLayout(webViewConfiguration: webViewConfiguration)
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        setupNavigationBar()
        setupTitleObservation()
        setupPageBackgroundObservation()
        setupJSEngine()
        setContentScrollView(rootView.webView.scrollView, for: .top)
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)

        setupWebViewLayout()
    }

    override func viewIsAppearing(_ animated: Bool) {
        super.viewIsAppearing(animated)

        guard isEnteringScreen else { return }

        presenter.didBecomeVisible()
    }

    override func updateContentUnavailableConfiguration(
        using _: UIContentUnavailableConfigurationState
    ) {
        guard let loadFailure else {
            contentUnavailableConfiguration = nil
            return
        }

        contentUnavailableConfiguration = UIContentUnavailableConfiguration.titleSubtitle(
            with: loadFailure.title,
            subtitle: loadFailure.message
        )
    }

    deinit {
        titleObservation?.invalidate()
        pageBackgroundObservations.forEach { $0.invalidate() }
        pendingFailureTask?.cancel()
    }
}

// MARK: - Private

private extension SPAViewController {
    enum Constants {
        static let minimumLoadingDuration: TimeInterval = 0.5
    }

    /// The computed backgrounds of the root element and the body, outermost first, taken on the next frame so a
    /// restyle in progress has settled.
    static let pageBackgroundScript = """
    return await new Promise(resolve => requestAnimationFrame(() => resolve([
        getComputedStyle(document.documentElement).backgroundColor,
        document.body ? getComputedStyle(document.body).backgroundColor : 'rgba(0, 0, 0, 0)'
    ])));
    """

    func display(loadFailure content: ErrorContent) {
        rootView.activityIndicatorView.stopAnimating()
        loadIndicator.stop()

        loadFailure = content
        setNeedsUpdateContentUnavailableConfiguration()
    }

    func setupJSEngine() {
        let navigationHandler: SPANavigationDecisionHandling =
            switch configuration.contentSource {
            case .dotNs,
                 .bundled:
                DotNsNavigationDecisionHandler(
                    baseHost: configuration.page.host,
                    hostProvider: hostProvider
                )
            case let .directURL(url):
                DirectURLNavigationDecisionHandler(baseURL: url)
            #if DEBUG
                case let .devServer(url, _):
                    DirectURLNavigationDecisionHandler(baseURL: url)
            #endif
            }

        let engine = SPAJSEngine(
            webView: rootView.webView,
            navigationDecisionHandler: navigationHandler,
            logger: logger
        )

        engine.onNavigationIntercepted = { [weak self] url in
            self?.presenter.didInterceptNavigation(to: url)
        }

        engine.onDeeplinkRequested = { [weak self] url in
            self?.presenter.didRequestDeeplink(url)
        }

        presenter.setup(engine: engine)
    }

    func setupNavigationBar() {
        if configuration.showMoreButton, configuration.presentation != .browserTab {
            setupMoreButton()
        }
    }

    func setupMoreButton() {
        let button = UIBarButtonItem(
            image: UIImage(systemName: "ellipsis"),
            style: .plain,
            target: self,
            action: #selector(onMoreTapped)
        )
        button.tintColor = .fgPrimary
        navigationItem.rightBarButtonItem = button
    }

    func setupTitleObservation() {
        guard configuration.title == nil, !configuration.isRootScreen else { return }

        titleObservation = rootView.webView.observe(\.title, options: .new) { [weak self] webView, _ in
            guard let title = webView.title, !title.isEmpty else { return }

            Task { @MainActor in
                self?.presenter.didUpdateWebViewTitle(title)
            }
        }
    }

    /// The page's background is read again whenever it can have changed: a load finishes, a loaded page moves to
    /// another address, or the app's theme changes and a page that follows it restyles without a reload.
    func setupPageBackgroundObservation() {
        let webView = rootView.webView
        pageBackgroundObservations = [
            webView.observe(\.isLoading, options: .new) { [weak self] webView, _ in
                guard !webView.isLoading else { return }

                MainActor.assumeIsolated { self?.readPageBackground() }
            },
            webView.observe(\.url, options: .new) { [weak self] webView, _ in
                guard !webView.isLoading else { return }

                MainActor.assumeIsolated { self?.readPageBackground() }
            }
        ]

        registerForTraitChanges(
            [DSThemeTrait.self, UITraitUserInterfaceStyle.self]
        ) { (controller: SPAViewController, _) in
            // The page restyles after the change reaches the web view. The tone read before that is stale, so
            // the strip keeps the app's theme until the page is read again.
            controller.setPageBackgroundTone(nil)
            DispatchQueue.main.async { [weak controller] in
                controller?.readPageBackground()
            }
        }
    }

    func readPageBackground() {
        rootView.webView.callAsyncJavaScript(
            Self.pageBackgroundScript,
            in: nil,
            in: .page
        ) { [weak self] result in
            // A read cut off by a navigation fails after the next page's read has answered.
            guard let cssColors = (try? result.get()) as? [String] else { return }

            self?.setPageBackgroundTone(PageBackgroundTone(cssColors: cssColors))
        }
    }

    func setPageBackgroundTone(_ tone: PageBackgroundTone?) {
        guard tone != pageBackgroundTone else { return }

        pageBackgroundTone = tone
        onStatusStripThemeChanged?()
    }

    func setupWebViewLayout() {
        guard !didSetupWebViewLayout else { return }
        didSetupWebViewLayout = true

        // The Browse page does not read the safe area yet, so the host insets it.
        rootView.setupWebViewLayout(pageReadsSafeArea: !configuration.isRootScreen)
    }

    // MARK: - Actions

    @objc func onMoreTapped() {
        presenter.didTapMoreButton()
    }
}

// MARK: - SPAViewProtocol

extension SPAViewController: SPAViewProtocol {
    var pageScrollView: UIScrollView { rootView.webView.scrollView }
    var pageWebView: WKWebView { rootView.webView }

    func navigate(to url: URL) {
        rootView.webView.load(URLRequest(url: url))
    }

    func navigate(to page: ProductPage) {
        guard let currentURL = rootView.webView.url else {
            return
        }

        navigate(to: page.applied(to: currentURL))
    }

    func updateTitle(_ title: String) {
        navigationItem.title = title
        statusBarTitle = title
        onStatusBarTitleChanged?()
    }

    func reload() {
        rootView.webView.reload()
    }

    func showLoading() {
        rootView.activityIndicatorView.startAnimating()
        loadIndicator.start()

        pendingFailureTask?.cancel()
        pendingFailureTask = nil
        loadingStartedAt = .now

        guard loadFailure != nil else { return }

        loadFailure = nil
        setNeedsUpdateContentUnavailableConfiguration()
    }

    func hideLoading() {
        rootView.activityIndicatorView.stopAnimating()
        loadIndicator.finish()
    }

    /// A failure arriving right after a retry would swap the error state out and back in the same
    /// beat, so the loading state is held for a minimum time before the error replaces it.
    func showLoadFailure(_ content: ErrorContent) {
        let elapsed = Date.now.timeIntervalSince(loadingStartedAt ?? .distantPast)

        pendingFailureTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(max(0, Constants.minimumLoadingDuration - elapsed)))

            guard !Task.isCancelled else { return }

            self?.display(loadFailure: content)
        }
    }

    func updateLoadPhase(_ phase: ProductLoadPhase) {
        loadIndicator.update(phase)
    }
}

// MARK: - NavigationBarHiding

extension SPAViewController: NavigationBarHiding {
    /// The Browse tab's root shows no bar. The status strip carries the tab's name.
    var hidesNavigationBar: Bool { configuration.isRootScreen }
}

// MARK: - StatusStripThemeProviding

extension SPAViewController: StatusStripThemeProviding {
    /// A page draws under the status strip in its own colours. Where they clash with the app's theme the strip
    /// takes a theme that reads on the page.
    var statusStripTheme: ThemeSelection? {
        let appThemeIsLight = UIColor.bgSurfaceMain.resolvedColor(with: traitCollection).isLight
        return pageBackgroundTone?.statusStripTheme(appThemeIsLight: appThemeIsLight)
    }
}

// MARK: - StatusBarTitleProviding

extension SPAViewController: StatusBarTitleProviding {
    func makeStatusBarMenu() -> UIMenu {
        UIMenu(children: [
            UIDeferredMenuElement.uncached { [weak self] completion in
                completion(self?.presenter.makeMenuElements() ?? [])
            }
        ])
    }
}
