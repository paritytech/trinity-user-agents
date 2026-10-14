import Foundation
import WebKit

final class SchemeHandlerProxy: NSObject, WKURLSchemeHandler {
    private var handler: (any WKURLSchemeHandler)?
    private var pendingTasks: [(WKWebView, any WKURLSchemeTask)] = []

    @MainActor
    func setHandler(_ handler: any WKURLSchemeHandler) {
        self.handler = handler
        pendingTasks.forEach { handler.webView($0, start: $1) }
        pendingTasks.removeAll()
    }

    @MainActor
    func webView(
        _ webView: WKWebView,
        start urlSchemeTask: any WKURLSchemeTask
    ) {
        guard let handler else {
            pendingTasks.append((webView, urlSchemeTask))
            return
        }

        handler.webView(webView, start: urlSchemeTask)
    }

    @MainActor
    func webView(
        _ webView: WKWebView,
        stop urlSchemeTask: any WKURLSchemeTask
    ) {
        guard let handler else {
            pendingTasks.removeAll { ($1 as AnyObject) === (urlSchemeTask as AnyObject) }
            return
        }

        handler.webView(webView, stop: urlSchemeTask)
    }
}
