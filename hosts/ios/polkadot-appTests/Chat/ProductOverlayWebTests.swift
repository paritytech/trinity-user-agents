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
}

private extension ProductOverlayWebViewTests {
    static let donePage = """
    <!doctype html><html><body style="background:transparent"><script>
    setTimeout(function () { webkit.messageHandlers.productOverlay.postMessage({ kind: 'overlayDone' }); }, 200);
    </script></body></html>
    """
    static let stuckPage = #"<!doctype html><html><body style="background:transparent"></body></html>"#

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
