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
    private let url: URL
    private let returnURL: URL?
    private let webView: WKWebView
    private let origin = FundingWebPageOrigin()
    private var completion: ((FundingFrameOutcome) -> Void)?
    private var urlObservation: NSKeyValueObservation?
    private var secureObservation: NSKeyValueObservation?

    init(url: URL, returnURL: URL?, completion: @escaping (FundingFrameOutcome) -> Void) {
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
        origin.host = FundingReturnURL.displayOrigin(of: url)
        urlObservation = webView.observe(\.url, options: [.new]) { [weak self] webView, _ in
            MainActor.assumeIsolated {
                guard let self, let current = webView.url else { return }
                self.origin.host = FundingReturnURL.displayOrigin(of: current)
                self.refreshSecurity()
            }
        }
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

    private func refreshSecurity() {
        origin.isSecure = webView.url?.scheme?.lowercased() == "https" && webView.hasOnlySecureContent
    }

    /// Answers the core once and takes the frame down.
    func finish(_ outcome: FundingFrameOutcome) {
        guard let completion else { return }
        self.completion = nil

        webView.stopLoading()
        if presentingViewController != nil, !isBeingDismissed {
            dismiss(animated: true)
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
        if isMainFrame, let returnURL, FundingReturnURL.matches(target, returnURL: returnURL) {
            decisionHandler(.cancel)
            finish(.returned(url: target.absoluteString))
            return
        }

        decisionHandler(FundingReturnURL.allowsNavigation(to: target, mainFrame: isMainFrame) ? .allow : .cancel)
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
           FundingReturnURL.allowsNavigation(to: target, mainFrame: true) {
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
        let isPageOrigin = origin.protocol == "https"
            && webView.url?.scheme == "https"
            && webView.url?.host?.lowercased() == origin.host.lowercased()
        decisionHandler(isPageOrigin ? .grant : .deny)
    }
}

/// How a funding page's address is read: whether navigation reached the
/// return URL, and the origin the bar shows.
enum FundingReturnURL {
    /// Whether `url` is `returnURL`: the same scheme, host, port and path,
    /// whatever its query or fragment.
    static func matches(_ url: URL, returnURL: URL) -> Bool {
        guard let candidate = URLComponents(url: url, resolvingAgainstBaseURL: false),
              let target = URLComponents(url: returnURL, resolvingAgainstBaseURL: false),
              let scheme = candidate.scheme?.lowercased(),
              let host = candidate.host?.lowercased()
        else { return false }

        return scheme == target.scheme?.lowercased()
            && host == target.host?.lowercased()
            && port(of: candidate) == port(of: target)
            && path(of: candidate) == path(of: target)
    }

    /// Whether a frame may load `url`. The page itself loads only https; a
    /// frame inside it may also hold the inert documents a page builds for
    /// itself (`about:`, `blob:`, `data:`).
    static func allowsNavigation(to url: URL, mainFrame: Bool) -> Bool {
        switch url.scheme?.lowercased() {
        case "https": true
        case "about", "blob", "data": !mainFrame
        default: false
        }
    }

    /// The host, with its port when it is not the scheme's default.
    static func displayOrigin(of url: URL) -> String {
        guard let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
              let host = components.host
        else { return url.absoluteString }

        guard let port = components.port, port != defaultPort(for: components.scheme) else { return host }
        return "\(host):\(port)"
    }
}

private extension FundingReturnURL {
    static func port(of components: URLComponents) -> Int? {
        components.port ?? defaultPort(for: components.scheme)
    }

    static func defaultPort(for scheme: String?) -> Int? {
        switch scheme?.lowercased() {
        case "https": 443
        case "http": 80
        default: nil
        }
    }

    static func path(of components: URLComponents) -> String {
        components.percentEncodedPath.isEmpty ? "/" : components.percentEncodedPath
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
