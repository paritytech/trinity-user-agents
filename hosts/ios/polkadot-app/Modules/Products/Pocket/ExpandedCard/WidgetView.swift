import DesignSystem
import PolkadotUI
import Products
import func TrUAPIHost.productContentScheme
import SnapKit
import UIKit
import UIKitExt
import WebKit

/// A Pocket card's page in a transparent web view over the whole screen. The card face drawn
/// underneath is part of the view's safe area, which the page reads as `env(safe-area-inset-top)`.
final class WidgetView: UIView {
    let webView: WKWebView

    /// The page's scroll offset, which the face follows.
    var onScroll: ((CGFloat) -> Void)?

    /// Set by the screen, since the safe area the page sees includes the face the indicator runs beside.
    var scrollIndicatorInsets: UIEdgeInsets {
        get { webView.scrollView.verticalScrollIndicatorInsets }
        set { webView.scrollView.verticalScrollIndicatorInsets = newValue }
    }

    private let loadingView = PolkadotLogoLoadingView()
    private let loadProgressView: UIProgressView = {
        let progressView = UIProgressView(progressViewStyle: .bar)
        progressView.progressTintColor = .bgAccent
        progressView.trackTintColor = .clear
        progressView.isHidden = true
        return progressView
    }()

    private let loadStatusLabel: Label = {
        let label = Label()
        label.typography = .bodyMedium
        label.usesMonospacedDigits = true
        label.textColor = .fgSecondary
        label.textAlignment = .center
        label.numberOfLines = 0
        label.isHidden = true
        return label
    }()

    private lazy var loadIndicator = SPALoadIndicator(progressView: loadProgressView, statusLabel: loadStatusLabel)
    private var failureView: UIContentUnavailableView?
    private var offsetObservation: NSKeyValueObservation?

    init(schemeHandler: SchemeHandlerProxy) {
        webView = WKWebView(frame: .zero, configuration: Self.makeConfiguration(schemeHandler: schemeHandler))
        super.init(frame: .zero)

        setupWebView()
        setupLayout()
        observeScroll()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    deinit {
        offsetObservation?.invalidate()
    }

    func showLoading() {
        hideFailure()
        loadingView.startAnimating()
        loadIndicator.start()
    }

    func updateLoadPhase(_ phase: ProductLoadPhase) {
        loadIndicator.update(phase)
    }

    func hideLoading() {
        loadingView.stopAnimating()
        loadIndicator.finish()
    }

    func showFailure(_ content: ErrorContent) {
        loadingView.stopAnimating()
        loadIndicator.stop()
        hideFailure()

        let failureView = UIContentUnavailableView(
            configuration: .titleSubtitle(with: content.title, subtitle: content.message)
        )
        addSubview(failureView)
        failureView.snp.makeConstraints { make in
            make.edges.equalTo(safeAreaLayoutGuide)
        }
        self.failureView = failureView
    }
}

private extension WidgetView {
    static func makeConfiguration(schemeHandler: SchemeHandlerProxy) -> WKWebViewConfiguration {
        let configuration = WKWebViewConfiguration()
        configuration.applicationNameForUserAgent = ProductUserAgent.applicationName
        configuration.allowsInlineMediaPlayback = true
        configuration.mediaTypesRequiringUserActionForPlayback = []
        configuration.setURLSchemeHandler(schemeHandler, forURLScheme: productContentScheme())
        return configuration
    }

    func setupWebView() {
        backgroundColor = .clear
        webView.isOpaque = false
        webView.backgroundColor = .clear
        webView.scrollView.backgroundColor = .clear
        webView.scrollView.contentInsetAdjustmentBehavior = .never
        webView.scrollView.automaticallyAdjustsScrollIndicatorInsets = false

        #if DEBUG || E2E_TEST
            webView.isInspectable = true
        #endif
    }

    func setupLayout() {
        addSubview(webView)
        webView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        addSubview(loadProgressView)
        loadProgressView.snp.makeConstraints { make in
            make.top.leading.trailing.equalTo(safeAreaLayoutGuide)
        }

        addSubview(loadingView)
        loadingView.snp.makeConstraints { make in
            make.center.equalTo(safeAreaLayoutGuide)
        }

        addSubview(loadStatusLabel)
        loadStatusLabel.snp.makeConstraints { make in
            make.top.equalTo(loadingView.snp.bottom).offset(16)
            make.leading.trailing.equalTo(safeAreaLayoutGuide).inset(32)
        }
    }

    func observeScroll() {
        offsetObservation = webView.scrollView.observe(\.contentOffset, options: .new) { [weak self] _, change in
            guard let offset = change.newValue else { return }

            MainActor.assumeIsolated { self?.onScroll?(offset.y) }
        }
    }

    func hideFailure() {
        failureView?.removeFromSuperview()
        failureView = nil
    }
}
