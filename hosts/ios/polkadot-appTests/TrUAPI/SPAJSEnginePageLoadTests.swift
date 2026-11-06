import Foundation
import SDKLogger
import Testing
import WebKit
@testable import polkadot_app

@MainActor
struct SPAJSEnginePageLoadTests {
    @Test func aFinishedPageLoaded() {
        let page = PageUnderTest()

        page.engine.webView(page.webView, didFinish: nil)

        #expect(page.ends == [true])
    }

    @Test(arguments: [
        (NSURLErrorDomain, NSURLErrorCancelled),
        ("WebKitErrorDomain", 102),
    ])
    func aLoadANewerOneReplacedDidNotFail(domain: String, code: Int) {
        let page = PageUnderTest()
        let superseded = NSError(domain: domain, code: code)

        page.engine.webView(page.webView, didFail: nil, withError: superseded)
        page.engine.webView(page.webView, didFailProvisionalNavigation: nil, withError: superseded)

        #expect(page.ends.isEmpty)
    }

    @Test func aLoadThatFailedDidNotLoad() {
        let page = PageUnderTest()
        let failure = NSError(domain: NSURLErrorDomain, code: NSURLErrorTimedOut)

        page.engine.webView(page.webView, didFail: nil, withError: failure)
        page.engine.webView(page.webView, didFailProvisionalNavigation: nil, withError: failure)

        #expect(page.ends == [false, false])
    }

    /// WebKit reloads the page after its process dies, and that reload must not pass for the first load.
    @Test func aWebContentProcessThatDiedFailsTheLoad() {
        let page = PageUnderTest()

        page.engine.webViewWebContentProcessDidTerminate(page.webView)

        #expect(page.ends == [false])
    }

    @Test(arguments: [
        (200, true, true),
        (404, true, false),
        (500, false, true),
    ])
    func aMainFrameHTTPErrorFailsTheLoad(status: Int, mainFrame: Bool, loaded: Bool) throws {
        let page = PageUnderTest()
        let response = try StubNavigationResponse(status: status, mainFrame: mainFrame)
        var policy: WKNavigationResponsePolicy?

        page.engine.webView(page.webView, decidePolicyFor: response) { policy = $0 }
        page.engine.webView(page.webView, didFinish: nil)

        #expect(policy == .allow)
        #expect(page.ends == [loaded])
    }
}

@MainActor
private final class PageUnderTest {
    let webView = WKWebView(frame: .zero)
    let engine: SPAJSEngine
    private(set) var ends: [Bool] = []

    init() {
        engine = SPAJSEngine(webView: webView, logger: Logger.shared)
        engine.onPageLoadFinished = { [weak self] loaded in self?.ends.append(loaded) }
    }
}

private final class StubNavigationResponse: WKNavigationResponse {
    private let stubbedResponse: URLResponse
    private let mainFrame: Bool

    init(status: Int, mainFrame: Bool) throws {
        let url = try #require(URL(string: "http://localhost:3000"))
        stubbedResponse = try #require(HTTPURLResponse(url: url, statusCode: status, httpVersion: nil, headerFields: nil))
        self.mainFrame = mainFrame
        super.init()
    }

    override var response: URLResponse { stubbedResponse }

    override var isForMainFrame: Bool { mainFrame }
}
