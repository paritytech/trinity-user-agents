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

    /// The web view covers the whole screen, under the bars and the Docklet. With `pageReadsSafeArea` the scroll
    /// view takes no insets from the host: the page reads the safe area through `env(safe-area-inset-*)` and insets
    /// its own content, which needs `viewport-fit=cover` in its viewport. Without it the host insets the page's
    /// scrolling content by the safe area.
    func setupWebViewLayout(pageReadsSafeArea: Bool) {
        addSubview(webView)
        webView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        setupActivityIndicatorLayout()
        setupLoadProgressLayout()
        webView.scrollView.contentInsetAdjustmentBehavior = pageReadsSafeArea ? .never : .scrollableAxes
    }
}

// MARK: - Private

private extension SPAViewLayout {
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

    func setupLoadProgressLayout() {
        addSubview(loadProgressView)
        loadProgressView.snp.makeConstraints { make in
            make.top.equalTo(safeAreaLayoutGuide.snp.top)
            make.leading.trailing.equalToSuperview()
        }
    }
}
