import DesignSystem
import PolkadotUI
import Products
import func TrUAPIHost.productContentScheme
import SnapKit
import UIKit
import UIKitExt
import WebKit

/// A Pocket card's page in a transparent web view over the whole screen. The page starts its
/// content at `cardInset` and leaves the area above it to the card face drawn underneath.
final class WidgetView: UIView {
    let webView: WKWebView

    /// The page's scroll offset, which the face follows.
    var onScroll: ((CGFloat) -> Void)?
    var onRetry: (() -> Void)?

    private let cardInset: CGFloat
    private let contentArea = UILayoutGuide()
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

    init(cardInset: CGFloat, schemeHandler: SchemeHandlerProxy) {
        self.cardInset = cardInset
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

        var configuration = UIContentUnavailableConfiguration.empty()
        configuration.text = content.title
        configuration.secondaryText = content.message
        configuration.button.title = String(localized: .Common.retry)
        configuration.buttonProperties.primaryAction = UIAction { [weak self] _ in self?.onRetry?() }

        let failureView = UIContentUnavailableView(configuration: configuration)
        addSubview(failureView)
        failureView.snp.makeConstraints { make in
            make.edges.equalTo(contentArea)
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

        #if DEBUG || E2E_TEST
            webView.isInspectable = true
        #endif
    }

    func setupLayout() {
        addSubview(webView)
        webView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        addLayoutGuide(contentArea)
        contentArea.snp.makeConstraints { make in
            make.top.equalToSuperview().offset(cardInset)
            make.leading.trailing.bottom.equalToSuperview()
        }

        addSubview(loadProgressView)
        loadProgressView.snp.makeConstraints { make in
            make.top.leading.trailing.equalTo(contentArea)
        }

        addSubview(loadingView)
        loadingView.snp.makeConstraints { make in
            make.center.equalTo(contentArea)
        }

        addSubview(loadStatusLabel)
        loadStatusLabel.snp.makeConstraints { make in
            make.top.equalTo(loadingView.snp.bottom).offset(16)
            make.leading.trailing.equalTo(contentArea).inset(32)
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
