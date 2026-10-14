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

    let loadProgressView: UIProgressView = {
        let progressView = UIProgressView(progressViewStyle: .bar)
        progressView.progressTintColor = .bgAccent
        progressView.trackTintColor = .clear
        progressView.isHidden = true
        return progressView
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
}

// MARK: - Private

private extension SPASimplifiedViewLayout {
    func setupActivityIndicatorLayout() {
        addSubview(activityIndicatorView)
        activityIndicatorView.snp.makeConstraints { make in
            make.center.equalToSuperview()
        }
    }

    func setupLoadProgressLayout() {
        addSubview(loadProgressView)
        loadProgressView.snp.makeConstraints { make in
            make.top.equalTo(safeAreaLayoutGuide.snp.top)
            make.leading.trailing.equalToSuperview()
        }
    }
}
