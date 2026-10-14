#if canImport(WebKit)

import WebKit

public extension TrUAPIHost {
    /// Installs the bridge and shared container before loading a product.
    @MainActor
    static func installProductScripts(
        into webView: WKWebView,
        endpoint: WsBridgeEndpoint
    ) throws {
        let container = try ContainerScriptBundle.load()
        let controller = webView.configuration.userContentController
        controller.addUserScript(WKUserScript(
            source: LocalhostBridgeBootstrap.script(
                port: endpoint.port, token: endpoint.token
            ),
            injectionTime: .atDocumentStart,
            forMainFrameOnly: true
        ))
        controller.addUserScript(WKUserScript(
            source: container,
            injectionTime: .atDocumentStart,
            forMainFrameOnly: false
        ))
    }
}

#endif
