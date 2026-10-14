import DesignSystem
import FoundationExt
import PolkadotUI
import Products
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

    private var titleObservation: NSKeyValueObservation?
    private var didSetupWebViewLayout = false
    private var loadFailure: ErrorContent?
    private var loadingStartedAt: Date?
    private var pendingFailureTask: Task<Void, Never>?
    private var chromeCollapser: BrowserChromeCollapser?

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

        webViewConfiguration.allowsInlineMediaPlayback = true
        webViewConfiguration.mediaTypesRequiringUserActionForPlayback = []

        webViewConfiguration.setURLSchemeHandler(
            schemeHandlerProxy,
            forURLScheme: ProductScriptSchemeHandler.scheme
        )
        view = SPAViewLayout(webViewConfiguration: webViewConfiguration)
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        setupNavigationBar()
        setupTitleObservation()
        setupJSEngine()

        if configuration.isBrowserTab {
            setupBrowserTabActions()
            setupChromeCollapse()
        }
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)

        setupWebViewLayout()
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
            subtitle: loadFailure.message,
            actionTitle: String(localized: .Common.retry)
        ) { [weak self] in
            self?.presenter.didTapRetry()
        }
    }

    deinit {
        titleObservation?.invalidate()
        pendingFailureTask?.cancel()
    }
}

// MARK: - Private

private extension SPAViewController {
    enum Constants {
        static let minimumLoadingDuration: TimeInterval = 0.5
    }

    func display(loadFailure content: ErrorContent) {
        rootView.activityIndicatorView.stopAnimating()

        loadFailure = content
        setNeedsUpdateContentUnavailableConfiguration()
    }

    func setupJSEngine() {
        let navigationHandler: SPANavigationDecisionHandling =
            switch configuration.contentSource {
            case .dotNs:
                DotNsNavigationDecisionHandler(
                    baseHost: configuration.page.host,
                    hostProvider: hostProvider
                )
            case let .directURL(url):
                DirectURLNavigationDecisionHandler(baseURL: url)
            }

        let engine = SPAJSEngine(
            webView: rootView.webView,
            navigationDecisionHandler: navigationHandler,
            logger: logger
        )

        engine.onNavigationIntercepted = { [weak self] url in
            self?.presenter.didInterceptNavigation(to: url)
        }

        presenter.setup(engine: engine)
    }

    func setupNavigationBar() {
        if configuration.isRootScreen {
            setTitle(String(localized: .tabBrowse))
        }

        if configuration.showMoreButton, !configuration.isBrowserTab {
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

    func setupWebViewLayout() {
        guard !didSetupWebViewLayout else { return }
        didSetupWebViewLayout = true

        if configuration.isBrowserTab {
            rootView.setupBrowserTabLayout()
        } else if configuration.isRootScreen {
            rootView.setupRootLayout()
        } else {
            rootView.setupDefaultLayout()
        }
    }

    func setupBrowserTabActions() {
        rootView.minimizeButton.addTarget(self, action: #selector(onMinimizeTapped), for: .touchUpInside)
        rootView.moreButton.showsMenuAsPrimaryAction = true
        rootView.moreButton.menu = UIMenu(children: [
            UIDeferredMenuElement.uncached { [weak self] completion in
                completion(self?.makeMoreMenuElements() ?? [])
            }
        ])
    }

    func makeMoreMenuElements() -> [UIMenuElement] {
        var elements: [UIMenuElement] = []

        if presenter.hasChatEntry() {
            elements.append(UIAction(
                title: String(localized: .spaActionOpenChat),
                image: UIImage(systemName: "bubble.left")
            ) { [weak self] _ in
                self?.presenter.didTapOpenChat()
            })
        }

        elements.append(UIAction(
            title: String(localized: .spaActionRefresh),
            image: UIImage(systemName: "arrow.clockwise")
        ) { [weak self] _ in
            self?.reload()
        })

        elements.append(UIAction(
            title: String(localized: .spaActionShare),
            image: UIImage(systemName: "square.and.arrow.up")
        ) { [weak self] _ in
            self?.presenter.didTapShare()
        })

        elements.append(UIAction(
            title: String(localized: .Common.close),
            image: UIImage(systemName: "xmark"),
            attributes: .destructive
        ) { [weak self] _ in
            self?.presenter.didTapClose()
        })

        return elements
    }

    func setupChromeCollapse() {
        let collapser = BrowserChromeCollapser(topPanelHeight: rootView.topChromeHeight)
        chromeCollapser = collapser
        rootView.webView.scrollView.delegate = self
    }

    func feedChromeCollapse(_ scrollView: UIScrollView) {
        guard let collapser = chromeCollapser else { return }

        let sample = BrowserChromeScrollSample(
            offsetToTopEdge: max(0, scrollView.bounds.minY),
            isInteracting: scrollView.isDragging || scrollView.isDecelerating
        )

        if let update = collapser.update(with: sample) {
            rootView.applyChromeCollapse(update.fraction, animated: update.animated)
        }
    }

    func resetChromeCollapse() {
        guard let chromeCollapser else { return }

        chromeCollapser.reset()
        rootView.applyChromeCollapse(0, animated: false)
    }

    // MARK: - Actions

    @objc func onMoreTapped() {
        presenter.didTapMoreButton()
    }

    func showProgressBar() {
        rootView.loadProgressView.isHidden = false
    }

    func hideProgressBar(delay: TimeInterval = 0) {
        let closure = { [rootView] in
            rootView.loadProgressView.isHidden = true
            rootView.loadProgressView.setProgress(0, animated: false)
        }
        guard delay > 0 else {
            closure()
            return
        }

        DispatchQueue.main.asyncAfter(deadline: .now() + delay) {
            closure()
        }
    }

    @objc func onMinimizeTapped() {
        presenter.didTapMinimize()
    }
}

// MARK: - SPAViewProtocol

extension SPAViewController: SPAViewProtocol {
    var pageScrollView: UIScrollView { rootView.webView.scrollView }

    func navigate(to url: URL) {
        resetChromeCollapse()
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
    }

    func reload() {
        resetChromeCollapse()
        rootView.webView.reload()
    }

    func showLoading() {
        rootView.activityIndicatorView.startAnimating()

        pendingFailureTask?.cancel()
        pendingFailureTask = nil
        loadingStartedAt = .now

        guard loadFailure != nil else { return }

        loadFailure = nil
        setNeedsUpdateContentUnavailableConfiguration()
    }

    func hideLoading() {
        rootView.activityIndicatorView.stopAnimating()
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

    // Resolving: 0-10%
    // Downloading: 10-90%
    // Unpacking: 90-100%
    func updateLoadProgress(_ progress: DotNsLoadProgress) {
        switch progress {
        case .resolved:
            showProgressBar()
            rootView.loadProgressView.setProgress(0.1, animated: true)
        case let .downloading(fraction):
            showProgressBar()
            rootView.loadProgressView.setProgress(0.1 + 0.8 * Float(fraction), animated: true)
        case .unpacking:
            showProgressBar()
            rootView.loadProgressView.setProgress(0.9, animated: true)
        case .completed:
            rootView.loadProgressView.setProgress(1.0, animated: true)
            hideProgressBar(delay: 0.3)
        case .idle,
             .failed:
            hideProgressBar()
        }
    }
}

// MARK: - RootScreen

extension SPAViewController: RootScreen {}

// MARK: - UIScrollViewDelegate

extension SPAViewController: UIScrollViewDelegate {
    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        feedChromeCollapse(scrollView)
    }

    func scrollViewDidEndDragging(_ scrollView: UIScrollView, willDecelerate decelerate: Bool) {
        if !decelerate {
            feedChromeCollapse(scrollView)
        }
    }

    func scrollViewDidEndDecelerating(_ scrollView: UIScrollView) {
        feedChromeCollapse(scrollView)
    }
}
