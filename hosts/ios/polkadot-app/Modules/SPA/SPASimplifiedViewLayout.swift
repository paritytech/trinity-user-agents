import DesignSystem
import SnapKit
import UIKit
import WebKit
import PolkadotUI

/// Layout for builds without the browse tab: no floating top pills and no chrome collapse, so the
/// browser tab layout is just the web view pinned edge to edge.
final class SPASimplifiedViewLayout: UIView {
    let webView: WKWebView

    let activityIndicatorView = PolkadotLogoLoadingView()

    let statusBarTitleButton = StatusBarTitleButton()
    private let statusStrip = UIView()
    private let chainStatusAnchorGuide = UILayoutGuide()
    private var chainStatusAnchorWidth: Constraint?

    let loadProgressView: UIProgressView = {
        let progressView = UIProgressView(progressViewStyle: .bar)
        progressView.progressTintColor = .bgAccent
        progressView.trackTintColor = .clear
        progressView.isHidden = true
        return progressView
    }()

    let loadStatusLabel: Label = {
        let label = Label()
        label.typography = .bodyMedium
        label.usesMonospacedDigits = true
        label.textColor = .fgSecondary
        label.textAlignment = .center
        label.numberOfLines = 0
        label.isHidden = true
        return label
    }()

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

        webView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
        webView.scrollView.contentInsetAdjustmentBehavior = .automatic

        setupActivityIndicatorLayout()
        setupLoadProgressLayout()

        setNeedsLayout()
    }

    /// The strip floats over the top of the edge-to-edge web view, whose content is inset below it.
    func setupFullScreenLayout(chainStatusView: UIView) {
        addSubview(webView)
        webView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
        webView.scrollView.contentInsetAdjustmentBehavior = .automatic
        webView.scrollView.contentInset.top = ChainConnectionStatusBarView.preferredHeight
        webView.scrollView.verticalScrollIndicatorInsets.top = ChainConnectionStatusBarView.preferredHeight

        addSubview(statusStrip)
        statusStrip.snp.makeConstraints { make in
            make.top.equalTo(safeAreaLayoutGuide.snp.top)
            make.leading.trailing.equalToSuperview()
            make.height.equalTo(ChainConnectionStatusBarView.preferredHeight)
        }

        statusStrip.addSubview(chainStatusView)
        chainStatusView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        statusStrip.addLayoutGuide(chainStatusAnchorGuide)
        chainStatusAnchorGuide.snp.makeConstraints { make in
            make.trailing.equalToSuperview().offset(-ChainConnectionStatusBarView.horizontalPadding)
            make.top.bottom.equalToSuperview()
            chainStatusAnchorWidth = make.width.equalTo(1).constraint
        }

        statusStrip.addSubview(statusBarTitleButton)
        statusBarTitleButton.snp.makeConstraints { make in
            make.leading.equalToSuperview().offset(ChainConnectionStatusBarView.horizontalPadding)
            make.centerY.equalToSuperview()
            make.height.equalTo(ChainConnectionStatusBarView.preferredHeight)
            make.trailing.lessThanOrEqualTo(chainStatusAnchorGuide.snp.leading).offset(-DSSpacings.small)
        }

        setupActivityIndicatorLayout()
        setupLoadProgressLayout(top: statusStrip.snp.bottom)

        if #available(iOS 26.0, *) {
            let interaction = UIScrollEdgeElementContainerInteraction()
            interaction.edge = .top
            interaction.scrollView = webView.scrollView
            statusStrip.addInteraction(interaction)
        }
    }

    /// Sizes the space the status title leaves for the rings, `ringsWidth` wide at the trailing end.
    func setChainStatusWidth(_ ringsWidth: CGFloat) {
        chainStatusAnchorWidth?.update(offset: ringsWidth)
    }
}

// MARK: - Private

private extension SPASimplifiedViewLayout {
    func setupActivityIndicatorLayout() {
        addSubview(activityIndicatorView)
        activityIndicatorView.snp.makeConstraints { make in
            make.center.equalToSuperview()
        }

        addSubview(loadStatusLabel)
        loadStatusLabel.snp.remakeConstraints { make in
            make.top.equalTo(activityIndicatorView.snp.bottom).offset(16)
            make.leading.trailing.equalToSuperview().inset(32)
        }
    }

    func setupLoadProgressLayout(top: ConstraintItem? = nil) {
        addSubview(loadProgressView)
        loadProgressView.snp.makeConstraints { make in
            make.top.equalTo(top ?? safeAreaLayoutGuide.snp.top)
            make.leading.trailing.equalToSuperview()
        }
    }
}
