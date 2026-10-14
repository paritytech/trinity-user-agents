import DesignSystem
import SnapKit
import UIKit
import WebKit
import PolkadotUI

final class SPAViewLayout: UIView {
    let webView: WKWebView

    let activityIndicatorView = PolkadotLogoLoadingView()

    let loadProgressView: UIProgressView = {
        let progressView = UIProgressView(progressViewStyle: .bar)
        progressView.progressTintColor = .bgAccent
        progressView.trackTintColor = .clear
        progressView.isHidden = true
        return progressView
    }()

    // Top pills exist only where products live on as tabs; elsewhere the sheet itself dismisses them.
    let moreButton = SPAViewLayout.makeIconButton(systemName: "ellipsis")
    let minimizeButton = SPAViewLayout.makeIconButton(systemName: "chevron.down")

    let minimizeContainer = UIView()
    let moreContainer = UIView()

    var topChromeHeight: CGFloat { Constants.topEdgeGap * 2 + Constants.topPillSize }

    private var isBrowserToolbarLayout = false
    private var collapseFraction: CGFloat = 0
    private var isSettling = false

    init(webViewConfiguration: WKWebViewConfiguration) {
        webView = WKWebView(frame: .zero, configuration: webViewConfiguration)
        super.init(frame: .zero)

        backgroundColor = .bgSurfaceMain
        webView.isOpaque = false
        webView.backgroundColor = .bgSurfaceMain

        // Expose the SPA WebView to Safari Web Inspector / ios-webkit-debug-proxy
        // for local DEBUG development and for the triangle-e2e artifact
        // (E2E_TEST), which attaches Playwright over CDP against host-playground
        // content. Compiled out of Release / Nightly device builds, so no
        // production exposure.
        #if DEBUG || E2E_TEST
            webView.isInspectable = true
        #endif
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()

        guard isBrowserToolbarLayout, !isSettling else { return }
        layoutWebViewFrame()
    }

    func setupRootLayout() {
        addSubview(webView)
        webView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        setupActivityIndicatorLayout()
        setupLoadProgressLayout()
        webView.scrollView.contentInsetAdjustmentBehavior = .scrollableAxes
    }

    func setupDefaultLayout() {
        addSubview(webView)
        webView.snp.makeConstraints { make in
            make.top.equalTo(safeAreaLayoutGuide.snp.top)
            make.leading.trailing.bottom.equalToSuperview()
        }

        setupActivityIndicatorLayout()
        setupLoadProgressLayout()
        webView.scrollView.contentInsetAdjustmentBehavior = .never
    }

    func setupBrowserTabLayout() {
        addSubview(webView)

        // The pills float above the web view, so its frame follows the chrome collapse.
        isBrowserToolbarLayout = true
        webView.scrollView.contentInsetAdjustmentBehavior = .never
        setupTopPills()
        setupAccessibility()

        setupActivityIndicatorLayout()
        setupLoadProgressLayout()

        setNeedsLayout()
    }

    func applyChromeCollapse(_ fraction: CGFloat, animated: Bool) {
        collapseFraction = fraction

        let apply = { [self] in
            let sideScale = max(0.001, 1.0 - fraction)
            let sideTransform = CGAffineTransform(scaleX: sideScale, y: sideScale)
            minimizeContainer.transform = sideTransform
            minimizeContainer.alpha = 1.0 - fraction
            moreContainer.transform = sideTransform
            moreContainer.alpha = 1.0 - fraction

            layoutWebViewFrame()
        }

        if animated {
            isSettling = true
            UIView.animate(
                withDuration: Constants.settleDuration,
                delay: 0,
                options: [.curveEaseInOut, .beginFromCurrentState, .allowUserInteraction],
                animations: apply
            ) { [weak self] _ in
                self?.isSettling = false
                self?.setNeedsLayout()
            }
        } else {
            apply()
        }
    }
}

// MARK: - Private

private extension SPAViewLayout {
    func layoutWebViewFrame() {
        let expandedTop = safeAreaInsets.top + topChromeHeight
        let collapsedTop = safeAreaInsets.top + Constants.collapsedTopHeight
        let top = expandedTop + (collapsedTop - expandedTop) * collapseFraction

        webView.frame = CGRect(x: 0, y: top, width: bounds.width, height: max(0, bounds.height - top))
    }

    func setupActivityIndicatorLayout() {
        addSubview(activityIndicatorView)
        activityIndicatorView.snp.makeConstraints { make in
            make.center.equalToSuperview()
        }
    }

    private func setupLoadProgressLayout() {
        addSubview(loadProgressView)
        loadProgressView.snp.makeConstraints { make in
            make.top.equalTo(safeAreaLayoutGuide.snp.top)
            make.leading.trailing.equalToSuperview()
        }
    }

    func setupTopPills() {
        setupTopPill(minimizeContainer, hosting: minimizeButton)
        setupTopPill(moreContainer, hosting: moreButton)

        minimizeContainer.snp.makeConstraints { make in
            make.leading.equalTo(safeAreaLayoutGuide).offset(Constants.topSideInset)
            make.top.equalTo(safeAreaLayoutGuide.snp.top).offset(Constants.topEdgeGap)
            make.size.equalTo(Constants.topPillSize)
        }

        moreContainer.snp.makeConstraints { make in
            make.trailing.equalTo(safeAreaLayoutGuide).offset(-Constants.topSideInset)
            make.centerY.equalTo(minimizeContainer)
            make.size.equalTo(Constants.topPillSize)
        }
    }

    func setupTopPill(_ container: UIView, hosting button: UIButton) {
        addSubview(container)
        let background = DarkGlassPanelView(cornerRadius: Constants.topPillSize / 2)
        container.addSubview(background)
        background.snp.makeConstraints { make in make.edges.equalToSuperview() }
        container.addSubview(button)
        button.snp.makeConstraints { make in make.edges.equalToSuperview() }
    }

    func setupAccessibility() {
        moreButton.accessibilityLabel = String(localized: .Products.productBrowserAccessibilityMore)
        minimizeButton.accessibilityLabel = String(localized: .Products.productBrowserAccessibilityMinimize)
    }

    static func makeIconButton(systemName: String) -> UIButton {
        let button = UIButton(type: .system)
        button.setImage(UIImage(systemName: systemName), for: .normal)
        button.tintColor = .fgStaticWhite
        return button
    }
}

// MARK: - Constants

private extension SPAViewLayout {
    enum Constants {
        static let topPillSize: CGFloat = 44.0
        static let topSideInset: CGFloat = DSSpacings.mediumIncreased
        static let topEdgeGap: CGFloat = DSSpacings.small
        static let collapsedTopHeight: CGFloat = 0.0
        static let settleDuration: TimeInterval = 0.25
    }
}
