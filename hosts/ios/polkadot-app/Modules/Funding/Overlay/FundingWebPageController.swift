import DesignSystem
import Observation
import PolkadotUI
import SwiftUI
import TrUAPIHost
import UIKit
import WebKit

/// An external https page, such as an onramp's widget, full screen over the
/// app with the page's origin in the host's own bar.
///
/// An onramp hands over to the provider behind it, so the page moves between
/// origins and the bar follows it. The page and every window it opens stay on
/// https: a navigation anywhere else is refused, and the bar's padlock shows
/// only while the page on screen loaded entirely over https. The page reaching
/// the return URL closes the frame and answers `Returned` with the full URL
/// navigated to. Closing it, or the frame going off screen any other way,
/// answers `Dismissed`.
final class FundingWebPageController: UIViewController {
    let providerId: ProductId
    private let url: URL
    private let returnURL: URL?
    private let webView: WKWebView
    private let origin = FundingWebPageOrigin()
    private var completion: ((FundingFrameOutcome) -> Void)?
    private var secureObservation: NSKeyValueObservation?

    init(providerId: ProductId, url: URL, returnURL: URL?, completion: @escaping (FundingFrameOutcome) -> Void) {
        self.providerId = providerId
        self.url = url
        self.returnURL = returnURL
        self.completion = completion

        let configuration = WKWebViewConfiguration()
        configuration.allowsInlineMediaPlayback = true
        configuration.preferences.javaScriptCanOpenWindowsAutomatically = true
        webView = WKWebView(frame: .zero, configuration: configuration)

        super.init(nibName: nil, bundle: nil)
        modalPresentationStyle = .fullScreen
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .bgSurfaceMain

        let topBar = UIHostingController(
            rootView: FundingWebPageTopBar(origin: origin) { [weak self] in self?.finish(.dismissed) }
        )
        topBar.view.backgroundColor = .clear
        addChild(topBar)
        topBar.view.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(topBar.view)
        topBar.didMove(toParent: self)

        webView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(webView)

        NSLayoutConstraint.activate([
            topBar.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            topBar.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            topBar.view.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
            topBar.view.heightAnchor.constraint(equalToConstant: Self.topBarHeight),
            webView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            webView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            webView.topAnchor.constraint(equalTo: topBar.view.bottomAnchor),
            webView.bottomAnchor.constraint(equalTo: view.bottomAnchor)
        ])

        webView.navigationDelegate = self
        webView.uiDelegate = self
        origin.host = Self.originLabel(of: url)
        secureObservation = webView.observe(\.hasOnlySecureContent, options: [.new]) { [weak self] _, _ in
            MainActor.assumeIsolated { self?.refreshSecurity() }
        }
        webView.load(URLRequest(url: url))
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        if isBeingDismissed || presentingViewController == nil {
            finish(.dismissed)
        }
    }

    /// The bar shows the page on screen: it follows a navigation once the page
    /// commits, never while a slower one is still loading over the old page.
    private func showCommittedPage() {
        if let committed = webView.url {
            origin.host = Self.originLabel(of: committed)
        }
        refreshSecurity()
    }

    private func refreshSecurity() {
        origin.isSecure = webView.url?.scheme?.lowercased() == "https" && webView.hasOnlySecureContent
    }

    /// Answers the core once and takes the frame down, then runs `dismissed`.
    func finish(_ outcome: FundingFrameOutcome, dismissed: (() -> Void)? = nil) {
        guard let completion else { return }
        self.completion = nil

        webView.stopLoading()
        if presentingViewController != nil, !isBeingDismissed {
            dismiss(animated: true, completion: dismissed)
        } else {
            dismissed?()
        }
        completion(outcome)
    }
}

extension FundingWebPageController: WKNavigationDelegate {
    func webView(
        _: WKWebView,
        decidePolicyFor navigationAction: WKNavigationAction,
        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
    ) {
        guard let target = navigationAction.request.url else {
            decisionHandler(.cancel)
            return
        }

        let isMainFrame = navigationAction.targetFrame?.isMainFrame ?? true
        if isMainFrame, let returnURL,
           fundingPageMatchesReturn(url: target.absoluteString, returnUrl: returnURL.absoluteString) {
            decisionHandler(.cancel)
            finish(.returned(url: target.absoluteString))
            return
        }

        let allowed = fundingPageAllowsNavigation(url: target.absoluteString, isMainFrame: isMainFrame)
        decisionHandler(allowed ? .allow : .cancel)
    }

    func webView(_: WKWebView, didCommit _: WKNavigation!) {
        showCommittedPage()
    }

    func webView(_: WKWebView, didFailProvisionalNavigation _: WKNavigation!, withError _: Error) {
        showCommittedPage()
    }

    func webView(_: WKWebView, didFail _: WKNavigation!, withError _: Error) {
        showCommittedPage()
    }
}

extension FundingWebPageController: WKUIDelegate {
    /// An https window the page opens, such as a card issuer's 3-D Secure
    /// check, loads in place of the page.
    func webView(
        _ webView: WKWebView,
        createWebViewWith _: WKWebViewConfiguration,
        for navigationAction: WKNavigationAction,
        windowFeatures _: WKWindowFeatures
    ) -> WKWebView? {
        if navigationAction.targetFrame == nil,
           let target = navigationAction.request.url,
           fundingPageAllowsNavigation(url: target.absoluteString, isMainFrame: true) {
            webView.load(navigationAction.request)
        }
        return nil
    }

    /// The camera and microphone go to the https page on screen, for an
    /// identity check's selfie, and to nothing else.
    func webView(
        _ webView: WKWebView,
        requestMediaCapturePermissionFor origin: WKSecurityOrigin,
        initiatedByFrame _: WKFrameInfo,
        type _: WKMediaCaptureType,
        decisionHandler: @escaping (WKPermissionDecision) -> Void
    ) {
        let port = origin.port == 0 ? "" : ":\(origin.port)"
        let requester = "\(origin.protocol)://\(origin.host)\(port)"
        let allowed = webView.url.map {
            fundingPageAllowsMedia(pageUrl: $0.absoluteString, origin: requester)
        } ?? false
        decisionHandler(allowed ? .grant : .deny)
    }
}

private extension FundingWebPageController {
    /// The bar's label for `url`'s origin, as the core reads it: the host,
    /// with its port when it is not the scheme's default.
    static func originLabel(of url: URL) -> String {
        guard let origin = fundingPageOrigin(url: url.absoluteString) else { return url.absoluteString }
        return origin.port.map { "\(origin.host):\($0)" } ?? origin.host
    }
}

private extension FundingWebPageController {
    static let topBarHeight: CGFloat = 56
}

@MainActor
@Observable
private final class FundingWebPageOrigin {
    var host = ""
    var isSecure = false
}

private struct FundingWebPageTopBar: View {
    let origin: FundingWebPageOrigin
    let onClose: () -> Void

    var body: some View {
        HStack(spacing: DSSpacings.small) {
            if origin.isSecure {
                Image(systemName: "lock.fill")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(.fgSecondary)
            }
            Text(origin.host)
                .typography(.bodyMedium)
                .foregroundStyle(.fgPrimary)
                .lineLimit(1)
                .truncationMode(.head)
            Spacer()
            FundingCircleButton(systemImage: "xmark", action: onClose)
        }
        .padding(.horizontal, DSSpacings.medium)
        .frame(maxHeight: .infinity)
    }
}
