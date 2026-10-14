import Foundation
import FoundationExt
import Products
import SDKLogger
import WebKit

final class SPAJSEngine: NSObject, @unchecked Sendable {
    private(set) var state: JSEngineState = .initializing

    let webView: WKWebView
    private weak var contentController: WKUserContentController?
    private var initContinuation: CheckedContinuation<Void, Error>?
    private var messageHandlers: [String: (Any) -> Void] = [:]

    private var jsDeviceCapabilityHandler: JSDeviceCapabilityHandler?

    private let logger: LoggerProtocol
    private let scriptHandlers: [JSEngineScriptHandling]
    private let navigationDecisionHandler: SPANavigationDecisionHandling?

    var onNavigationIntercepted: ((URL) -> Void)?

    init(
        webView: WKWebView,
        scriptHandlers: [JSEngineScriptHandling]? = nil,
        navigationDecisionHandler: SPANavigationDecisionHandling? = nil,
        logger: LoggerProtocol
    ) {
        self.webView = webView
        self.logger = logger
        self.scriptHandlers = scriptHandlers ?? [JSEngineLogger(logger: logger)]
        self.navigationDecisionHandler = navigationDecisionHandler
        super.init()

        for handler in self.scriptHandlers {
            messageHandlers[handler.handlerName] = { body in
                handler.handle(body: body)
            }
        }
    }
}

// MARK: - Private Setup

private extension SPAJSEngine {
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
                    self?.logger.error("handler '\(name)' failed: \(error)")
                }
            }
        }

        guard !alreadyRegistered else {
            return
        }

        contentController?.add(self, name: name)
    }

    @MainActor
    @discardableResult
    func performEvaluation(_ script: String) async throws -> Any? {
        guard state == .ready else {
            throw JSEngineError.notReady
        }

        return try await webView.evaluateJavaScript(script)
    }

    @MainActor
    func setupEngine(scripts: [JSEngineScript]) {
        let contentController = webView.configuration.userContentController
        self.contentController = contentController

        contentController.removeAllScriptMessageHandlers()
        contentController.removeAllUserScripts()

        for scriptHandler in scriptHandlers {
            let script = scriptHandler.getScript()
            let userScript = WKUserScript(
                source: script.content,
                injectionTime: script.insertionPoint.toWkInjectionTime,
                forMainFrameOnly: script.frameScope == .mainFrameOnly
            )
            contentController.addUserScript(userScript)
        }

        for script in scripts {
            let userScript = WKUserScript(
                source: script.content,
                injectionTime: script.insertionPoint.toWkInjectionTime,
                forMainFrameOnly: script.frameScope == .mainFrameOnly
            )
            contentController.addUserScript(userScript)
        }

        for name in messageHandlers.keys {
            contentController.add(self, name: name)
        }

        webView.navigationDelegate = self
        webView.uiDelegate = self

        state = .ready
        completeInitialization()
    }

    @MainActor
    func cleanup() {
        contentController?.removeAllScriptMessageHandlers()
        webView.navigationDelegate = nil
        webView.uiDelegate = nil
        webView.stopLoading()
    }

    @MainActor
    func completeInitialization(error: Error? = nil) {
        guard let continuation = initContinuation else { return }
        initContinuation = nil

        if let error {
            state = .error(error.localizedDescription)
            continuation.resume(throwing: error)
        } else {
            continuation.resume()
        }
    }
}

// MARK: - JSEngineProtocol

extension SPAJSEngine: JSEngineProtocol {
    func initialize(with scripts: [JSEngineScript]) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            DispatchQueue.main.async {
                switch self.state {
                case .ready:
                    continuation.resume(returning: ())
                default:
                    self.state = .initializing
                    self.initContinuation = continuation
                    self.setupEngine(scripts: scripts)
                }
            }
        }
    }

    func getState() async -> JSEngineState {
        await performGetState()
    }

    @discardableResult
    func evaluate(_ script: String) async throws -> Any? {
        try await performEvaluation(script)
    }

    func registerFunction(name: String, handler: @escaping JSNativeHandler) async {
        await performRegisterFunction(name: name, handler: handler)
    }

    func dispatchEvent(actionId: String, payload: String) async throws {
        let escapedId = actionId.jsEscaped
        let escapedPayload = payload.jsEscaped
        try await performEvaluation("NativeBridge._dispatchEvent('\(escapedId)', '\(escapedPayload)')")
    }

    func destroy() async {
        await performDestroy()
    }

    func registerJSDeviceCapabilityHandler(
        _ handler: @escaping JSDeviceCapabilityHandler
    ) async {
        await MainActor.run {
            self.jsDeviceCapabilityHandler = handler
        }
    }
}

// MARK: - WKNavigationDelegate

extension SPAJSEngine: WKNavigationDelegate {
    func webView(
        _: WKWebView,
        decidePolicyFor navigationAction: WKNavigationAction,
        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
    ) {
        guard let navigationDecisionHandler else {
            decisionHandler(.allow)
            return
        }

        switch navigationDecisionHandler.decide(for: navigationAction) {
        case .allow:
            decisionHandler(.allow)
        case let .intercept(url):
            decisionHandler(.cancel)
            onNavigationIntercepted?(url)
        }
    }

    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        logger.error("WebView process terminated, attempting recovery reload")
        webView.reload()
    }
}

// MARK: - WKUIDelegate

extension SPAJSEngine: WKUIDelegate {
    func webView(
        _: WKWebView,
        decideMediaCapturePermissionsFor _: WKSecurityOrigin,
        initiatedBy _: WKFrameInfo,
        type: WKMediaCaptureType
    ) async -> WKPermissionDecision {
        guard let handler = jsDeviceCapabilityHandler else {
            logger.warning("SPAJSEngine: no device capability handler, denying media capture")
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
                logger.warning("SPAJSEngine: unknown media capture type, denying")
                return .deny
            }
        } catch {
            logger.error("SPAJSEngine: device capability handler failed: \(error)")
            return .deny
        }
    }
}

// MARK: - WKScriptMessageHandler

extension SPAJSEngine: WKScriptMessageHandler {
    func userContentController(
        _: WKUserContentController,
        didReceive message: WKScriptMessage
    ) {
        messageHandlers[message.name]?(message.body)
    }

    private static func stringBody(_ body: Any) -> String {
        if let string = body as? String {
            return string
        }
        return "\(body)"
    }
}
