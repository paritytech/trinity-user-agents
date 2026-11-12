import Foundation
import Products
import SDKLogger
import Testing
import WebKit
@testable import polkadot_app

@MainActor
struct PocketCardInsetScriptTests {
    @Test func pageReadsTheInsetAtDocumentStart() async throws {
        let webView = WKWebView(frame: .zero, configuration: WKWebViewConfiguration())
        let engine = SPAJSEngine(webView: webView, logger: Logger.shared)
        try await engine.initialize(with: [.pocketCardInset(300)])
        try await load(webView, html: "<html><body></body></html>")

        let inset = try await webView.evaluateJavaScript(
            "getComputedStyle(document.documentElement).getPropertyValue('--pocket-card-inset').trim()"
        )

        #expect(inset as? String == "300.0px")
        await engine.destroy()
    }

    private func load(_ webView: WKWebView, html: String) async throws {
        let observer = LoadObserver()
        let previous = webView.navigationDelegate
        webView.navigationDelegate = observer
        defer { webView.navigationDelegate = previous }
        try await withCheckedThrowingContinuation { continuation in
            observer.continuation = continuation
            webView.loadHTMLString(html, baseURL: URL(string: "https://stash.test/"))
        }
    }
}
