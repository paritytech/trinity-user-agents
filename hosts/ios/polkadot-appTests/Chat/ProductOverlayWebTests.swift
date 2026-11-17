import Foundation
import Testing
import struct TrUAPIHost.ProductId
import UIKit
import WebKit
@testable import polkadot_app

struct ProductOverlayMessageTests {
    @Test func thePageSaysItIsDone() {
        #expect(ProductOverlayMessage(body: ["kind": "overlayDone"] as [String: Any]) == .done)
        #expect(ProductOverlayMessage(body: #"{"kind":"overlayDone"}"#) == .done)
    }

    @Test func anythingElseIsDropped() {
        #expect(ProductOverlayMessage(body: ["kind": "done"] as [String: Any]) == nil)
        #expect(ProductOverlayMessage(body: 42) == nil)
        #expect(ProductOverlayMessage(body: "not json") == nil)
        // An overlay page cannot act for the product or open another overlay.
        #expect(ProductOverlayMessage(body: ["kind": "action", "actionId": "jollity.register"] as [String: Any]) == nil)
        #expect(ProductOverlayMessage(body: ["kind": "sendOverlay", "path": "a.html"] as [String: Any]) == nil)
    }
}

struct ProductOverlayPageTests {
    private let product = try! ProductId("overlay-test.testnet")

    @Test func aPageOnTheProductOriginIsShown() throws {
        let url = try #require(ProductOverlayWebView.pageURL(
            path: "fx/confetti-overlay.html?seed=1",
            productId: product
        ))

        #expect(url.host == "overlay-test.testnet")
        #expect(url.path == "/fx/confetti-overlay.html")
        #expect(url.query == "seed=1")
    }

    /// The content handler answers any of these with the product's whole app, so none may become an overlay.
    @Test func aPathThatNamesNoFileIsRefused() {
        let paths = ["?x", "#x", "?seed=1#x", "confetti", "fx/", "fx/confetti", "fx.d/confetti", "confetti."]
        for path in paths {
            #expect(ProductOverlayWebView.pageURL(path: path, productId: product) == nil, "\(path)")
        }
    }

    @Test func aPathOffTheProductOriginIsRefused() {
        let paths = [
            "",
            " ",
            "/confetti.html",
            "//evil.invalid/x.html",
            "https://evil.invalid/x.html",
            "javascript:x.js"
        ]
        for path in paths {
            #expect(ProductOverlayWebView.pageURL(path: path, productId: product) == nil, "\(path)")
        }
    }
}

@MainActor
@Suite(.serialized)
struct ProductOverlayWebViewTests {
    private let product = try! ProductId("overlay-test.testnet")
    private let window: UIWindow = {
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 390, height: 844))
        window.isHidden = false
        return window
    }()

    @Test func itCoversTheWindowTakesNoTouchAndGoesWhenItsPageIsDone() async throws {
        let chat = UIView(frame: window.bounds)
        window.addSubview(chat)
        let overlay = try #require(show(page: Self.donePage))

        #expect(overlay.frame == window.bounds)
        #expect(!overlay.isUserInteractionEnabled)
        #expect(overlay.accessibilityElementsHidden)
        #expect(window.subviews.last === overlay)
        #expect(window.hitTest(CGPoint(x: 195, y: 422), with: nil) === chat)

        #expect(await waitUntil(seconds: 10) { !overlay.isShowing }, "the overlay stayed after its page was done")
    }

    @Test func onlyOneShowsAtATime() throws {
        let first = try #require(show(page: Self.stuckPage))

        #expect(show(page: Self.donePage) == nil)
        first.dismiss()
        #expect(show(page: Self.stuckPage) != nil)
    }

    @Test func aPageThatNeverEndsGoesAtTheLimit() async throws {
        let overlay = try #require(show(page: Self.stuckPage, limit: .milliseconds(300)))

        #expect(await waitUntil(seconds: 5) { !overlay.isShowing }, "the overlay outlived its limit")
    }

    /// The footer page asks for a bare query first, which would be the product's whole app, then for a real page.
    /// Only the real page shows, served by the footer's own content handler.
    @Test func theFooterShowsOnlyAnOverlayThatNamesAPage() async throws {
        let content = PagesSchemeHandler(pages: [
            "/footer.html": Self.footerAskingForOverlays,
            "/overlay.html": Self.stuckPage
        ])
        let footer = try ProductRoomFooterWebView(
            pageURL: #require(ProductRoomFooterWebPage.url(path: "footer.html", productId: product)),
            productId: product,
            height: 64,
            schemeHandler: content,
            onAction: { _, _ in true },
            logger: StubLogger()
        )
        footer.frame = CGRect(x: 0, y: 700, width: 390, height: 64)
        window.addSubview(footer)

        let shown = await waitUntil(seconds: 10) { overlay(in: window) != nil }
        #expect(shown, "the footer's overlay never showed")
        #expect(await waitUntil(seconds: 2) { content.requestedPaths.contains("/overlay.html") })
        #expect(content.requestedPaths == ["/footer.html", "/overlay.html"], "an overlay loaded something else")

        overlay(in: window)?.dismiss()
        footer.removeFromSuperview()
    }
}

private extension ProductOverlayWebViewTests {
    static let donePage = """
    <!doctype html><html><body style="background:transparent"><script>
    setTimeout(function () { webkit.messageHandlers.productOverlay.postMessage({ kind: 'overlayDone' }); }, 200);
    </script></body></html>
    """
    static let stuckPage = #"<!doctype html><html><body style="background:transparent"></body></html>"#
    static let footerAskingForOverlays = """
    <!doctype html><html><body><script>
    webkit.messageHandlers.productFooter.postMessage({ kind: 'sendOverlay', path: '?x' });
    webkit.messageHandlers.productFooter.postMessage({ kind: 'sendOverlay', path: 'overlay.html' });
    </script></body></html>
    """

    func overlay(in window: UIWindow) -> ProductOverlayWebView? {
        window.subviews.lazy.compactMap { $0 as? ProductOverlayWebView }.first
    }

    func show(page: String, limit: Duration = ProductOverlayWebView.limit) -> ProductOverlayWebView? {
        let url = ProductRoomFooterWebPage.url(path: "overlay.html", productId: product)!
        return ProductOverlayWebView.show(
            over: window,
            pageURL: url,
            productId: product,
            schemeHandler: OnePageSchemeHandler(html: page),
            limit: limit,
            logger: StubLogger()
        )
    }

    func waitUntil(seconds: Double, _ condition: () -> Bool) async -> Bool {
        let deadline = Date().addingTimeInterval(seconds)
        while Date() < deadline {
            if condition() { return true }
            try? await Task.sleep(for: .milliseconds(50))
        }
        return condition()
    }
}

/// Answers every request with the same page, as the product's content would answer its own path.
private final class OnePageSchemeHandler: NSObject, WKURLSchemeHandler {
    private let html: String

    init(html: String) {
        self.html = html
    }

    func webView(_: WKWebView, start urlSchemeTask: any WKURLSchemeTask) {
        let data = Data(html.utf8)
        let response = URLResponse(
            url: urlSchemeTask.request.url!,
            mimeType: "text/html",
            expectedContentLength: data.count,
            textEncodingName: "utf-8"
        )
        urlSchemeTask.didReceive(response)
        urlSchemeTask.didReceive(data)
        urlSchemeTask.didFinish()
    }

    func webView(_: WKWebView, stop _: any WKURLSchemeTask) {}
}

/// Answers the pages it has by path, fails the rest, and remembers every path asked for.
private final class PagesSchemeHandler: NSObject, WKURLSchemeHandler {
    private let pages: [String: String]
    private(set) var requestedPaths: [String] = []

    init(pages: [String: String]) {
        self.pages = pages
    }

    func webView(_: WKWebView, start urlSchemeTask: any WKURLSchemeTask) {
        let url = urlSchemeTask.request.url!
        requestedPaths.append(url.path)
        guard let html = pages[url.path] else {
            urlSchemeTask.didFailWithError(URLError(.fileDoesNotExist))
            return
        }
        let data = Data(html.utf8)
        let response = URLResponse(
            url: url,
            mimeType: "text/html",
            expectedContentLength: data.count,
            textEncodingName: "utf-8"
        )
        urlSchemeTask.didReceive(response)
        urlSchemeTask.didReceive(data)
        urlSchemeTask.didFinish()
    }

    func webView(_: WKWebView, stop _: any WKURLSchemeTask) {}
}
