import Foundation
import Products
import SDKLogger
import Testing
import WebKit
@testable import polkadot_app

/// Drives a real `WKWebView` so the motion delegate is proven to be the one
/// WebKit calls, answering `requestPermission()` without WebKit's own prompt.
@MainActor
struct SPAJSEngineMotionPermissionTests {
    @Test(arguments: [
        ("DeviceMotionEvent", JSDeviceCapabilityDecision.allowed, "granted"),
        ("DeviceMotionEvent", .denied, "denied"),
        ("DeviceOrientationEvent", .allowed, "granted"),
        ("DeviceOrientationEvent", .denied, "denied"),
    ])
    func requestPermissionIsAnsweredByHost(
        event: String,
        decision: JSDeviceCapabilityDecision,
        expected: String
    ) async throws {
        let requests = CapabilityLog()
        // WebKit caches the decision per origin in the data store, so each case
        // gets its own store to reach the delegate.
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        let webView = WKWebView(frame: .zero, configuration: configuration)
        let engine = SPAJSEngine(webView: webView, logger: Logger.shared)
        #expect(engine.responds(to: NSSelectorFromString(
            "webView:requestDeviceOrientationAndMotionPermissionForOrigin:initiatedByFrame:decisionHandler:"
        )))
        await engine.registerJSDeviceCapabilityHandler { capability in
            await requests.append(capability)
            return decision
        }
        try await engine.initialize(with: [])
        try await load(webView, html: "<html><body></body></html>")

        let result = try await webView.callAsyncJavaScript(
            "return await \(event).requestPermission()",
            contentWorld: .page
        )

        #expect(result as? String == expected)
        #expect(await requests.capabilities == [.motion])
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

private actor CapabilityLog {
    private(set) var capabilities: [JSDeviceCapability] = []

    func append(_ capability: JSDeviceCapability) {
        capabilities.append(capability)
    }
}

private final class LoadObserver: NSObject, WKNavigationDelegate {
    var continuation: CheckedContinuation<Void, Error>?

    func webView(_: WKWebView, didFinish _: WKNavigation!) {
        continuation?.resume()
        continuation = nil
    }

    func webView(_: WKWebView, didFail _: WKNavigation!, withError error: Error) {
        continuation?.resume(throwing: error)
        continuation = nil
    }

    func webView(_: WKWebView, didFailProvisionalNavigation _: WKNavigation!, withError error: Error) {
        continuation?.resume(throwing: error)
        continuation = nil
    }
}
