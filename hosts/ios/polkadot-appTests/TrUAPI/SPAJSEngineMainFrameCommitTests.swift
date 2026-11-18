import Foundation
import Products
import SDKLogger
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
import WebKit
@testable import polkadot_app

/// Drives a real `WKWebView`, so the commit is proven to reach a handler set through `JSEngineProtocol`, which
/// is all the runtime knows of its engine.
@MainActor
struct SPAJSEngineMainFrameCommitTests {
    @Test
    func aMainFrameCommitReachesTheHandlerSetThroughTheProtocol() async throws {
        let webView = WKWebView(frame: .zero, configuration: WKWebViewConfiguration())
        let engine: JSEngineProtocol = SPAJSEngine(webView: webView, logger: Logger.shared)
        let (commits, continuation) = AsyncStream<Void>.makeStream()

        await engine.onMainFrameCommit { continuation.yield() }
        try await engine.initialize(with: [])
        webView.loadHTMLString("<html><body></body></html>", baseURL: URL(string: "https://stash.test/"))

        try await withTimeout(.hangGuard) {
            for await _ in commits {
                return
            }
            throw CancellationError()
        }
        await engine.destroy()
    }
}
