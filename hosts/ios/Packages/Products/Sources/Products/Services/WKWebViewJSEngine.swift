import Foundation
import FoundationExt
import SDKLogger
import UIKit
import WebKit

public typealias WebViewHostWindowProvider = @MainActor () -> UIWindow?

public final class WKWebViewJSEngine: NSObject, @unchecked Sendable {
    private(set) var state: JSEngineState = .initializing

    // necessary to make sure iOS delivers runloop events to the web view
    private let hostWindowProvider: WebViewHostWindowProvider
    private var webView: WKWebView?
    private weak var contentController: WKUserContentController?
    private var initContinuation: CheckedContinuation<Void, Error>?

    private var messageHandlers: [String: (Any) -> Void] = [:]

    private var jsDeviceCapabilityHandler: JSDeviceCapabilityHandler?

    private let logger: SDKLoggerProtocol
    private let settings: WKWebViewJSEngineSettings
    private let scriptHandlers: [JSEngineScriptHandling]
    private let urlSchemeHandlers: [String: any WKURLSchemeHandler]
    private let engineBaseUrl: URL

    public init(
        engineBaseUrl: URL,
        scriptHandlers: [JSEngineScriptHandling],
        hostWindowProvider: @escaping WebViewHostWindowProvider,
        urlSchemeHandlers: [String: any WKURLSchemeHandler] = [:],
        settings: WKWebViewJSEngineSettings = WKWebViewJSEngineSettings(),
        logger: SDKLoggerProtocol
    ) {
        self.engineBaseUrl = engineBaseUrl
        self.scriptHandlers = scriptHandlers
        self.urlSchemeHandlers = urlSchemeHandlers
        self.settings = settings
        self.hostWindowProvider = hostWindowProvider
        self.logger = logger

        super.init()

        for scriptHandler in scriptHandlers {
            messageHandlers[scriptHandler.handlerName] = { body in
                scriptHandler.handle(body: body)
            }
        }
    }
}

private extension WKWebViewJSEngine {
    @MainActor
    func performGetState() -> JSEngineState {
        state
    }

    @MainActor
    func performDestroy() {
        state = .destroyed
        cleanup()
    }

    @MainActor
    func performRegisterFunction(name: String, handler: @escaping JSNativeHandler) {
        let alreadyRegistered = messageHandlers[name] != nil

        messageHandlers[name] = { [weak self] body in
            let stringValue = Self.stringBody(body)
            Task {
                do {
                    try await handler(stringValue)
                } catch {
                    self?.logger.error("JSEngine: handler '\(name)' failed: \(error)")
                }
            }
        }

        guard !alreadyRegistered else {
            return
        }

        contentController?.add(self, name: name)
    }

    @MainActor
    @discardableResult private func performEvaluation(_ script: String) async throws -> Any? {
        guard state == .ready, let webView else {
            throw JSEngineError.notReady
        }

        return try await webView.evaluateJavaScript(script)
    }

    @MainActor
    private func setupWebView(scripts: [JSEngineScript]) {
        cleanup()

        let contentController = WKUserContentController()
        self.contentController = contentController

        for scriptHandler in scriptHandlers {
            let script = scriptHandler.getScript()

            let wkScript = WKUserScript(
                source: script.content,
                injectionTime: script.insertionPoint.toWkInjectionTime,
                forMainFrameOnly: script.frameScope == .mainFrameOnly
            )

            contentController.addUserScript(wkScript)
        }

        for script in scripts {
            let wkScript = WKUserScript(
                source: script.content,
                injectionTime: script.insertionPoint.toWkInjectionTime,
                forMainFrameOnly: script.frameScope == .mainFrameOnly
            )

            contentController.addUserScript(wkScript)
        }

        for name in messageHandlers.keys {
            contentController.add(self, name: name)
        }

        let config = WKWebViewConfiguration()
        config.userContentController = contentController

        for (scheme, handler) in urlSchemeHandlers {
            config.setURLSchemeHandler(handler, forURLScheme: scheme)
        }

        if !settings.usesPersistentLocalStorage {
            config.websiteDataStore = .nonPersistent()
        }

        let webView = WKWebView(
            frame: CGRect(x: -1, y: -1, width: 1, height: 1),
            configuration: config
        )
        webView.navigationDelegate = self
        webView.alpha = 0
        webView.isUserInteractionEnabled = false
        webView.uiDelegate = self

        self.webView = webView

        // Attach to a host window so iOS doesn't throttle JavaScript timers.
        // WKWebView with a nil `window` has its WebContent process demoted on real
        // devices, which causes setTimeout/setInterval callbacks to be delayed or
        // suspended entirely even while the app is in the foreground.
        attachToHostWindow(webView: webView)

        // Use a file baseURL so the page gets a non-opaque origin.
        // This enables localStorage, which product scripts require.
        webView.loadHTMLString(Self.htmlTemplate, baseURL: engineBaseUrl)
    }

    @MainActor
    private func attachToHostWindow(webView: WKWebView) {
        guard let hostWindow = hostWindowProvider() else {
            logger.warning(
                "JSEngine: no host window available; JS timers may be throttled on device"
            )
            return
        }

        hostWindow.addSubview(webView)
    }

    @MainActor
    private func cleanup() {
        contentController?.removeAllScriptMessageHandlers()
        webView?.navigationDelegate = nil
        webView?.uiDelegate = nil
        webView?.stopLoading()
        webView?.removeFromSuperview()
        webView = nil
    }

    @MainActor
    private func completeInitialization(error: Error? = nil) {
        guard let continuation = initContinuation else { return }
        initContinuation = nil

        if let error {
            state = .error(error.localizedDescription)
            continuation.resume(throwing: error)
        } else {
            state = .ready
            continuation.resume()
        }
    }
}

extension WKWebViewJSEngine: JSEngineProtocol {
    public func initialize(with scripts: [JSEngineScript]) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            DispatchQueue.main.async {
                switch self.state {
                case .ready:
                    continuation.resume(returning: ())
                default:
                    self.state = .initializing
                    self.initContinuation = continuation
                    self.setupWebView(scripts: scripts)
                }
            }
        }
    }

    public func getState() async -> JSEngineState {
        await performGetState()
    }

    @discardableResult
    public func evaluate(_ script: String) async throws -> Any? {
        try await performEvaluation(script)
    }

    public func registerFunction(name: String, handler: @escaping JSNativeHandler) async {
        await performRegisterFunction(name: name, handler: handler)
    }

    public func dispatchEvent(actionId: String, payload: String) async throws {
        let escapedId = actionId.jsEscaped
        let escapedPayload = payload.jsEscaped
        try await performEvaluation("NativeBridge._dispatchEvent('\(escapedId)', '\(escapedPayload)')")
    }

    public func destroy() async {
        await performDestroy()
    }

    public func registerJSDeviceCapabilityHandler(
        _ handler: @escaping JSDeviceCapabilityHandler
    ) async {
        await MainActor.run {
            self.jsDeviceCapabilityHandler = handler
        }
    }
}

// MARK: - WKNavigationDelegate

extension WKWebViewJSEngine: WKNavigationDelegate {
    public func webView(_: WKWebView, didFinish _: WKNavigation!) {
        completeInitialization()
    }

    public func webView(_: WKWebView, didFail _: WKNavigation!, withError error: Error) {
        logger.error("JSEngine navigation failed: \(error)")
        completeInitialization(error: error)
    }

    public func webView(_: WKWebView, didFailProvisionalNavigation _: WKNavigation!, withError error: Error) {
        logger.error("JSEngine provisional navigation failed: \(error)")
        completeInitialization(error: error)
    }

    public func webViewWebContentProcessDidTerminate(_: WKWebView) {
        logger.error("JSEngine WebView process terminated")
        state = .error("WebView process terminated")
    }
}

// MARK: - WKScriptMessageHandler

extension WKWebViewJSEngine: WKScriptMessageHandler {
    public func userContentController(
        _: WKUserContentController,
        didReceive message: WKScriptMessage
    ) {
        messageHandlers[message.name]?(message.body)
    }

    static func stringBody(_ body: Any) -> String {
        if let string = body as? String {
            return string
        }
        return "\(body)"
    }
}

// MARK: - WKUIDelegate

extension WKWebViewJSEngine: WKUIDelegate {
    public func webView(
        _: WKWebView,
        decideMediaCapturePermissionsFor _: WKSecurityOrigin,
        initiatedBy _: WKFrameInfo,
        type: WKMediaCaptureType
    ) async -> WKPermissionDecision {
        guard let handler = jsDeviceCapabilityHandler else {
            logger.warning("JSEngine: no device capability handler, denying media capture")
            return .deny
        }

        do {
            switch type {
            case .camera:
                let decision = try await handler(.camera)

                logger.debug("Camera decision: \(decision)")
                return decision.toWKPermission
            case .microphone:
                let decision = try await handler(.microphone)

                logger.debug("Microphone decision: \(decision)")
                return decision.toWKPermission
            case .cameraAndMicrophone:
                let micDecision = try await handler(.microphone)
                logger.debug("Mic decision: \(micDecision)")
                guard micDecision == .allowed else { return .deny }
                let camDecision = try await handler(.camera)
                logger.debug("Cam decision: \(camDecision)")
                return camDecision.toWKPermission
            @unknown default:
                logger.warning("JSEngine: unknown media capture type, denying")
                return .deny
            }
        } catch {
            logger.error("JSEngine: device capability handler failed: \(error)")
            return .deny
        }
    }
}

// MARK: - HTML Template

private extension WKWebViewJSEngine {
    static let htmlTemplate = """
    <!DOCTYPE html>
    <html>
    <head><meta charset="utf-8"></head>
    <body>
    <script>
    window.NativeBridge = {
        _callbacks: {},
        _dispatchEvent: function(id, payload) {
            var cb = this._callbacks[id];
            if (cb) cb(payload);
        }
    };
    </script>
    </body>
    </html>
    """
}

// MARK: - Errors

public enum JSEngineError: Error, LocalizedError {
    case notReady
    case moduleLoadFailed

    public var errorDescription: String? {
        switch self {
        case .notReady:
            "JS engine is not in ready state"
        case .moduleLoadFailed:
            "Failed to load ES module script"
        }
    }
}
