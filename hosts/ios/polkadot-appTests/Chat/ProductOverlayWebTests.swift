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

struct ProductOverlayTouchRuleTests {
    private let touch = ContinuousClock.now

    @Test func anOverlayJustAfterATouchOnTheFooterIsHonoured() {
        var rule = ProductOverlayTouchRule()
        rule.touchEnded(at: touch, insideFooter: true)

        #expect(rule.allowsOverlay(at: touch))
        #expect(rule.allowsOverlay(at: touch + .milliseconds(900)))
        #expect(rule.allowsOverlay(at: touch + ProductOverlayTouchRule.window))
    }

    @Test func anOverlayWithNoTouchIsDropped() {
        #expect(!ProductOverlayTouchRule().allowsOverlay(at: touch))
    }

    @Test func anOverlayAfterTheWindowIsDropped() {
        var rule = ProductOverlayTouchRule()
        rule.touchEnded(at: touch, insideFooter: true)

        #expect(!rule.allowsOverlay(at: touch + ProductOverlayTouchRule.window + .milliseconds(1)))
        #expect(!rule.allowsOverlay(at: touch + .seconds(10)))
    }

    @Test func aTouchThatEndedOutsideTheFooterDoesNotCount() {
        var rule = ProductOverlayTouchRule()
        rule.touchEnded(at: touch, insideFooter: false)

        #expect(!rule.allowsOverlay(at: touch + .milliseconds(100)))
    }

    @Test func aLaterTouchOutsideTheFooterKeepsTheEarlierOne() {
        var rule = ProductOverlayTouchRule()
        rule.touchEnded(at: touch, insideFooter: true)
        rule.touchEnded(at: touch + .milliseconds(500), insideFooter: false)

        #expect(rule.allowsOverlay(at: touch + .milliseconds(800)))
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

    /// Just after a touch on the footer, the page asks for a bare query first, which would be the product's whole
    /// app, then for a real page. Only the real page shows, served by the footer's own content handler.
    @Test func theFooterShowsAnOverlayJustAfterATouchOnIt() async throws {
        let (footer, content, asked) = try await makeFooter()

        footer.touchEnded(at: CGPoint(x: 195, y: 32))
        footer.send(.state("ask"))

        #expect(await waitUntil(seconds: 10) { overlay(in: window) != nil }, "the footer's overlay never showed")
        #expect(await waitUntil(seconds: 2) { content.requestedPaths.contains("/overlay.html") })
        #expect(content.requestedPaths == ["/footer.html", "/overlay.html"], "an overlay loaded something else")
        #expect(asked.on)

        overlay(in: window)?.dismiss()
        footer.removeFromSuperview()
    }

    @Test func theFooterDropsAnOverlayAskedForWithNoTouch() async throws {
        let (footer, content, asked) = try await makeFooter()

        footer.send(.state("ask"))

        #expect(await waitUntil(seconds: 10) { asked.on }, "the page never asked")
        #expect(overlay(in: window) == nil)
        #expect(content.requestedPaths == ["/footer.html"])
        footer.removeFromSuperview()
    }

    @Test func theFooterDropsAnOverlayAskedForAfterTheWindow() async throws {
        let (footer, content, asked) = try await makeFooter()

        footer.touchEnded(at: CGPoint(x: 195, y: 32))
        try await Task.sleep(for: ProductOverlayTouchRule.window + .milliseconds(200))
        footer.send(.state("ask"))

        #expect(await waitUntil(seconds: 10) { asked.on }, "the page never asked")
        #expect(overlay(in: window) == nil)
        #expect(content.requestedPaths == ["/footer.html"])
        footer.removeFromSuperview()
    }

    @Test func aTouchThatEndedOutsideTheFooterDoesNotCount() async throws {
        let (footer, content, asked) = try await makeFooter()

        // The finger went down on the footer and lifted above it, over the chat.
        footer.touchEnded(at: CGPoint(x: 195, y: -40))
        footer.send(.state("ask"))

        #expect(await waitUntil(seconds: 10) { asked.on }, "the page never asked")
        #expect(overlay(in: window) == nil)
        #expect(content.requestedPaths == ["/footer.html"])
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
    /// Asks for two overlays whenever the host tells it something, then reports it asked with an action.
    static let footerAskingForOverlays = """
    <!doctype html><html><body><script>
    var post = function (message) { webkit.messageHandlers.productFooter.postMessage(message); };
    window.productFooter = { receive: function () {
      post({ kind: 'sendOverlay', path: '?x' });
      post({ kind: 'sendOverlay', path: 'overlay.html' });
      post({ kind: 'action', actionId: 'asked' });
    } };
    post({ kind: 'ready' });
    </script></body></html>
    """

    /// A footer in the window whose page has loaded, its content handler, and whether its page has asked.
    func makeFooter() async throws -> (ProductRoomFooterWebView, PagesSchemeHandler, Flag) {
        let content = PagesSchemeHandler(pages: [
            "/footer.html": Self.footerAskingForOverlays,
            "/overlay.html": Self.stuckPage
        ])
        let asked = Flag()
        let footer = try ProductRoomFooterWebView(
            pageURL: #require(ProductRoomFooterWebPage.url(path: "footer.html", productId: product)),
            productId: product,
            height: 64,
            schemeHandler: content,
            onAction: { actionId, _ in
                if actionId == "asked" { asked.on = true }
                return true
            },
            logger: StubLogger()
        )
        footer.frame = CGRect(x: 0, y: 700, width: 390, height: 64)
        window.addSubview(footer)
        let revealed = Flag()
        footer.whenRevealed { revealed.on = true }
        #expect(await waitUntil(seconds: 10) { revealed.on }, "the footer page never loaded")
        return (footer, content, asked)
    }

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

@MainActor
private final class Flag {
    var on = false
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
