import DesignSystem
import Foundation
import PolkadotUI
import Products
import struct TrUAPIHost.ProductId
import func TrUAPIHost.productContentScheme
import UIKit
import WebKit

// MARK: - View

/// A webview where the composer would be, showing the product's own page.
///
/// The page draws on a transparent background over a backdrop in the chat's own colour, which
/// fills the slot and the strip below it down to the screen edge, so the chat scrolling under
/// the footer stays hidden, as it does under the composer. The webview stays within the slot.
///
/// The page reports through the `productFooter` message handler: an action goes
/// to the product as a press in the room footer, a haptic plays, `sendOverlay` shows another of
/// the product's pages over the whole window for a moment, and `ready` reveals it. The overlay
/// is a webview of its own; this one never leaves the slot.
@MainActor
final class ProductRoomFooterWebView: UIView {
    typealias ActionHandler = @MainActor (_ actionId: String, _ payload: String?) async -> Bool

    private enum Constants {
        static let revealDelay: Duration = .seconds(2)
    }

    let pageURL: URL
    private let productId: ProductId
    private let onAction: ActionHandler
    /// The product's content, shared with the overlays this page asks for, so an overlay neither opens the content
    /// again nor serves another version of it.
    private let schemeHandler: any WKURLSchemeHandler
    private let logger: LoggerProtocol
    private let webView: WKWebView
    private let backdrop = UIView()
    private let dragWinner = UIPanGestureRecognizer()
    private var heightConstraint: NSLayoutConstraint!
    private var revealFallback: Task<Void, Never>?
    private var lastActionAt: Date?
    private var isRevealed = false
    private var revealWaiters: [() -> Void] = []

    /// The chat around the slot, told when the page changes the slot's height so its bottom inset follows.
    weak var inputHandler: ChatInputHandling?

    init(
        pageURL: URL,
        productId: ProductId,
        height: UInt32,
        schemeHandler: any WKURLSchemeHandler,
        onAction: @escaping ActionHandler,
        logger: LoggerProtocol
    ) {
        self.pageURL = pageURL
        self.productId = productId
        self.onAction = onAction
        self.schemeHandler = schemeHandler
        self.logger = logger

        let configuration = WKWebViewConfiguration()
        configuration.applicationNameForUserAgent = ProductUserAgent.applicationName
        configuration.setURLSchemeHandler(schemeHandler, forURLScheme: productContentScheme())
        configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
        configuration.allowsInlineMediaPlayback = true
        configuration.suppressesIncrementalRendering = true
        configuration.userContentController.addUserScript(WKUserScript(
            source: Self.noSelectionScript,
            injectionTime: .atDocumentEnd,
            forMainFrameOnly: true
        ))
        webView = WKWebView(frame: .zero, configuration: configuration)

        super.init(frame: .zero)

        // The webview copied its configuration; the content controller is the one it kept.
        webView.configuration.userContentController.add(
            WeakScriptMessageHandler(self),
            contentWorld: .page,
            name: ProductFooterMessage.handlerName
        )
        setupViews(height: ProductRoomFooterWebPage.height(height))
        webView.load(URLRequest(url: pageURL))
        revealFallback = Task { [weak self] in
            try? await Task.sleep(for: Constants.revealDelay)
            self?.reveal()
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        // The slot ends at the bottom safe area; the backdrop also covers the home indicator
        // strip below it. A frame, not a constraint, so the slot's fitted height stays its own.
        let below = window?.safeAreaInsets.bottom ?? 0
        backdrop.frame = CGRect(x: 0, y: 0, width: bounds.width, height: bounds.height + below)
    }

    override func safeAreaInsetsDidChange() {
        super.safeAreaInsetsDidChange()
        setNeedsLayout()
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        setNeedsLayout()
        guard window != nil else { return }
        makeDragWin()
    }

    /// Runs `action` once the page shows: at once if it already does.
    func whenRevealed(_ action: @escaping () -> Void) {
        if isRevealed {
            action()
        } else {
            revealWaiters.append(action)
        }
    }

    /// Tells the page something, through `window.productFooter.receive` when it defines one.
    func send(_ event: ProductFooterHostEvent) {
        webView.evaluateJavaScript(event.script) { [logger] _, error in
            if let error { logger.debug("[truapi.product] web footer did not take \(event.json): \(error)") }
        }
    }

    func handle(_ message: ProductFooterMessage) {
        switch message {
        case let .action(actionId, payload):
            // One confirm per slide: a page that posts twice in a burst sends once.
            if let lastActionAt, Date().timeIntervalSince(lastActionAt) < 0.5 { return }
            lastActionAt = Date()
            Task { [weak self, onAction] in
                let delivered = await onAction(actionId, payload)
                self?.send(delivered ? .state("confirmed") : .reset)
            }
        case let .haptic(style):
            Self.playHaptic(style)
        case let .sendOverlay(path):
            showOverlay(path: path)
        case let .ready(height):
            if let height = ProductRoomFooterWebPage.height(reported: height), height != heightConstraint.constant {
                heightConstraint.constant = height
                inputHandler?.chatInputDidChange()
            }
            reveal()
        }
    }
}

private extension ProductRoomFooterWebView {
    /// Shows the product's page at `path` over the window, one at a time. It outlives this
    /// footer: a footer the product replaces mid-effect does not cut it short, and it always
    /// ends with its page or at its limit.
    func showOverlay(path: String) {
        guard let window else { return }
        guard let url = ProductRoomFooterWebPage.url(path: path, productId: productId) else {
            logger.warning("[truapi.product] web footer refused an overlay off the product origin: \(path)")
            return
        }
        ProductOverlayWebView.show(
            over: window,
            pageURL: url,
            productId: productId,
            schemeHandler: schemeHandler,
            logger: logger
        )
    }

    static let noSelectionScript = """
    (function () {
      var style = document.createElement('style');
      style.textContent = 'html,body{-webkit-user-select:none;-webkit-touch-callout:none;' +
        'background:transparent!important;}';
      (document.head || document.documentElement).appendChild(style);
    })();
    """

    func setupViews(height: CGFloat) {
        backgroundColor = .clear
        clipsToBounds = false

        backdrop.backgroundColor = .bgSurfaceMain
        backdrop.isUserInteractionEnabled = false
        addSubview(backdrop)

        webView.isOpaque = false
        webView.backgroundColor = .clear
        webView.underPageBackgroundColor = .clear
        webView.scrollView.backgroundColor = .clear
        webView.scrollView.isScrollEnabled = false
        webView.scrollView.bounces = false
        webView.scrollView.contentInsetAdjustmentBehavior = .never
        webView.allowsLinkPreview = false
        webView.allowsBackForwardNavigationGestures = false
        webView.navigationDelegate = self
        webView.uiDelegate = self
        webView.alpha = 0
        #if DEBUG
            webView.isInspectable = true
        #endif

        webView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(webView)
        heightConstraint = heightAnchor.constraint(equalToConstant: height)
        NSLayoutConstraint.activate([
            heightConstraint,
            webView.topAnchor.constraint(equalTo: topAnchor),
            webView.leadingAnchor.constraint(equalTo: leadingAnchor),
            webView.trailingAnchor.constraint(equalTo: trailingAnchor),
            webView.bottomAnchor.constraint(equalTo: bottomAnchor)
        ])

        // Watches touches that start in the footer without taking them from the page.
        dragWinner.cancelsTouchesInView = false
        dragWinner.delaysTouchesBegan = false
        dragWinner.delaysTouchesEnded = false
        dragWinner.delegate = self
        addGestureRecognizer(dragWinner)
    }

    func reveal() {
        revealFallback?.cancel()
        revealFallback = nil
        guard !isRevealed else { return }
        isRevealed = true
        UIView.animate(withDuration: 0.15) { self.webView.alpha = 1 }
        let waiters = revealWaiters
        revealWaiters = []
        waiters.forEach { $0() }
    }

    /// A drag that starts in the footer is the page's: the back swipe and any pan of the
    /// screens around it wait for this one to fail, which it never does while the finger is down.
    func makeDragWin() {
        var competing: [UIGestureRecognizer] = []
        var ancestor = superview
        while let view = ancestor {
            competing += (view.gestureRecognizers ?? []).filter { $0 is UIPanGestureRecognizer }
            ancestor = view.superview
        }
        var responder: UIResponder? = self
        while let next = responder?.next {
            if let navigation = next as? UINavigationController,
               let pop = navigation.interactivePopGestureRecognizer {
                competing.append(pop)
            }
            responder = next
        }
        for recognizer in competing where recognizer !== dragWinner {
            recognizer.require(toFail: dragWinner)
        }
    }

    static func playHaptic(_ style: String) {
        switch style {
        case "warning":
            UINotificationFeedbackGenerator().notificationOccurred(.warning)
        case "error":
            UINotificationFeedbackGenerator().notificationOccurred(.error)
        case "selection":
            UISelectionFeedbackGenerator().selectionChanged()
        case "light":
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
        case "medium":
            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
        case "heavy":
            UIImpactFeedbackGenerator(style: .heavy).impactOccurred()
        case "soft":
            UIImpactFeedbackGenerator(style: .soft).impactOccurred()
        case "rigid":
            UIImpactFeedbackGenerator(style: .rigid).impactOccurred()
        default:
            UINotificationFeedbackGenerator().notificationOccurred(.success)
        }
    }

    func receive(_ message: WKScriptMessage) {
        // Only the footer page itself speaks for the product: not a frame inside it, not another origin.
        guard message.frameInfo.isMainFrame,
              message.frameInfo.securityOrigin.protocol.lowercased() == productContentScheme().lowercased(),
              message.frameInfo.securityOrigin.host.lowercased() == productId.rawValue.lowercased() else {
            logger.warning("[truapi.product] web footer dropped a message from outside the product page")
            return
        }
        guard let parsed = ProductFooterMessage(body: message.body) else {
            logger.debug("[truapi.product] web footer dropped an unreadable message")
            return
        }
        handle(parsed)
    }
}

extension ProductRoomFooterWebView: UIGestureRecognizerDelegate {
    func gestureRecognizer(
        _: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith _: UIGestureRecognizer
    ) -> Bool {
        true
    }
}

extension ProductRoomFooterWebView: WKNavigationDelegate {
    func webView(
        _: WKWebView,
        decidePolicyFor navigationAction: WKNavigationAction,
        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
    ) {
        guard let url = navigationAction.request.url else {
            decisionHandler(.cancel)
            return
        }
        let allowed = ProductRoomFooterWebPage.isProductOrigin(url, productId: productId) ||
            url.absoluteString == "about:blank"
        if !allowed {
            logger.warning("[truapi.product] web footer refused to leave the product origin for \(url.absoluteString)")
        }
        decisionHandler(allowed ? .allow : .cancel)
    }

    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        webView.load(URLRequest(url: pageURL))
    }

    func webView(_: WKWebView, didFinish _: WKNavigation!) {
        reveal()
    }
}

extension ProductRoomFooterWebView: WKUIDelegate {
    func webView(
        _: WKWebView,
        createWebViewWith _: WKWebViewConfiguration,
        for _: WKNavigationAction,
        windowFeatures _: WKWindowFeatures
    ) -> WKWebView? {
        nil
    }
}

/// The content controller keeps its handlers strongly; this keeps the view out of that cycle.
private final class WeakScriptMessageHandler: NSObject, WKScriptMessageHandler {
    private weak var target: ProductRoomFooterWebView?

    init(_ target: ProductRoomFooterWebView) {
        self.target = target
    }

    @MainActor
    func userContentController(_: WKUserContentController, didReceive message: WKScriptMessage) {
        target?.receive(message)
    }
}
