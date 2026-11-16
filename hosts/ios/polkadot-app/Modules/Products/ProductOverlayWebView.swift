import Foundation
import Products
import struct TrUAPIHost.ProductId
import func TrUAPIHost.productContentScheme
import UIKit
import WebKit

// MARK: - Page to host

/// One message an overlay page posts through `window.webkit.messageHandlers.productOverlay`.
///
/// The only one is `{kind:"overlayDone"}`: the page has finished and the host takes the overlay away.
enum ProductOverlayMessage: Equatable {
    case done

    static let handlerName = "productOverlay"

    /// Reads a message posted as an object or as its JSON text. Anything else is dropped.
    init?(body: Any) {
        let fields: [String: Any]? = if let object = body as? [String: Any] {
            object
        } else if let text = body as? String {
            try? JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any]
        } else {
            nil
        }
        guard fields?["kind"] as? String == "overlayDone" else { return nil }
        self = .done
    }
}

// MARK: - View

/// A see-through webview over the whole window, showing one of the product's own pages for a
/// moment (confetti after a slide, say). It takes no touches, so the chat under it stays
/// usable, and VoiceOver skips it.
///
/// It goes away when the page posts `{kind:"overlayDone"}`, when its limit runs out, or when
/// `dismiss()` is called. Only one shows at a time in a window: `show` refuses a second one
/// while the first is up.
@MainActor
final class ProductOverlayWebView: UIView {
    nonisolated static let limit: Duration = .seconds(5)

    private let productId: ProductId
    private let logger: LoggerProtocol
    private let webView: WKWebView
    private var expiry: Task<Void, Never>?

    /// Shows the product page at `pageURL` over `window`, or nothing when the window already shows an overlay.
    @discardableResult
    static func show(
        over window: UIWindow,
        pageURL: URL,
        productId: ProductId,
        schemeHandler: any WKURLSchemeHandler,
        limit: Duration = limit,
        logger: LoggerProtocol
    ) -> ProductOverlayWebView? {
        guard !window.subviews.contains(where: { $0 is ProductOverlayWebView }) else {
            logger.debug("[truapi.product] overlay of \(productId.rawValue) refused: one is already showing")
            return nil
        }
        let overlay = ProductOverlayWebView(
            pageURL: pageURL,
            productId: productId,
            schemeHandler: schemeHandler,
            logger: logger
        )
        overlay.frame = window.bounds
        overlay.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        window.addSubview(overlay)
        overlay.expiry = Task { [weak overlay] in
            try? await Task.sleep(for: limit)
            guard !Task.isCancelled, let overlay else { return }
            overlay.logger.debug("[truapi.product] overlay of \(overlay.productId.rawValue) ran out of time")
            overlay.dismiss()
        }
        return overlay
    }

    private init(
        pageURL: URL,
        productId: ProductId,
        schemeHandler: any WKURLSchemeHandler,
        logger: LoggerProtocol
    ) {
        self.productId = productId
        self.logger = logger

        let configuration = WKWebViewConfiguration()
        configuration.applicationNameForUserAgent = ProductUserAgent.applicationName
        configuration.setURLSchemeHandler(schemeHandler, forURLScheme: productContentScheme())
        configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
        webView = WKWebView(frame: .zero, configuration: configuration)

        super.init(frame: .zero)

        webView.configuration.userContentController.add(
            WeakOverlayMessageHandler(self),
            contentWorld: .page,
            name: ProductOverlayMessage.handlerName
        )
        setupViews()
        webView.load(URLRequest(url: pageURL))
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// Whether it is still on screen.
    var isShowing: Bool {
        superview != nil
    }

    /// Takes the overlay away and stops its page. Safe to call more than once.
    func dismiss() {
        expiry?.cancel()
        expiry = nil
        webView.stopLoading()
        webView.configuration.userContentController.removeScriptMessageHandler(
            forName: ProductOverlayMessage.handlerName,
            contentWorld: .page
        )
        removeFromSuperview()
    }

    func handle(_ message: ProductOverlayMessage) {
        switch message {
        case .done:
            dismiss()
        }
    }

    // Every touch goes to what is under the overlay: the chat.
    override func hitTest(_: CGPoint, with _: UIEvent?) -> UIView? {
        nil
    }
}

private extension ProductOverlayWebView {
    func setupViews() {
        backgroundColor = .clear
        isUserInteractionEnabled = false
        accessibilityElementsHidden = true

        webView.isOpaque = false
        webView.backgroundColor = .clear
        webView.underPageBackgroundColor = .clear
        webView.scrollView.backgroundColor = .clear
        webView.scrollView.isScrollEnabled = false
        webView.scrollView.contentInsetAdjustmentBehavior = .never
        webView.isUserInteractionEnabled = false
        webView.navigationDelegate = self
        webView.uiDelegate = self
        #if DEBUG
            webView.isInspectable = true
        #endif

        webView.frame = bounds
        webView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        addSubview(webView)
    }

    func receive(_ message: WKScriptMessage) {
        // Only the overlay page itself speaks: not a frame inside it, not another origin.
        guard message.frameInfo.isMainFrame,
              message.frameInfo.securityOrigin.protocol.lowercased() == productContentScheme().lowercased(),
              message.frameInfo.securityOrigin.host.lowercased() == productId.rawValue.lowercased() else {
            logger.warning("[truapi.product] overlay dropped a message from outside the product page")
            return
        }
        guard let parsed = ProductOverlayMessage(body: message.body) else {
            logger.debug("[truapi.product] overlay dropped an unreadable message")
            return
        }
        handle(parsed)
    }
}

extension ProductOverlayWebView: WKNavigationDelegate {
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
            logger.warning("[truapi.product] overlay refused to leave the product origin for \(url.absoluteString)")
        }
        decisionHandler(allowed ? .allow : .cancel)
    }

    /// A moment's effect is not worth a reload: it just ends.
    func webViewWebContentProcessDidTerminate(_: WKWebView) {
        dismiss()
    }
}

extension ProductOverlayWebView: WKUIDelegate {
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
private final class WeakOverlayMessageHandler: NSObject, WKScriptMessageHandler {
    private weak var target: ProductOverlayWebView?

    init(_ target: ProductOverlayWebView) {
        self.target = target
    }

    @MainActor
    func userContentController(_: WKUserContentController, didReceive message: WKScriptMessage) {
        target?.receive(message)
    }
}
