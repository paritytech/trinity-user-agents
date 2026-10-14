import Foundation
import SDKLogger
import SubstrateSdk

// MARK: - Protocol

public protocol ProductsScriptExecutorProtocol: AnyObject {
    /// Initialize the bot: create engine, load container + product script, register handlers.
    func initializeBot(nativeApi: ProductsNativeApiProtocol) async throws

    /// Notify JS that the bot chat has been opened.
    func onBotStarted() async throws

    /// Forward a user text message to JS via `dispatchUserMessage`.
    func onUserMessage(text: String, roomId: String?) async throws

    /// Trigger widget rendering for a custom message.
    /// Returns a stream of SCALE-encoded widget hex updates.
    func renderMessage(
        messageId: String,
        messageType: String,
        messageData: Data
    ) async -> AsyncThrowingStream<String, Error>

    /// Dispatch a UI event (button click, text field change) back to JS.
    func dispatchEvent(roomId: String?, messageId: String, actionId: String, payload: String?) async

    /// Release all resources. The executor cannot be used after this.
    func dispose() async
}

// MARK: - Implementation

public actor ProductsScriptExecutor: ProductsScriptExecutorProtocol {
    private let productUrl: URL
    private let scriptsFactory: ChatScriptsMaking
    private let engineFactory: @Sendable () -> JSEngineProtocol
    private let logger: SDKLoggerProtocol

    private var engine: JSEngineProtocol?
    private var engineMonitor: JSEngineMonitor?
    private var containerBridge: ContainerBridge?
    private var moduleBridge: JSESModuleBridge?
    private var nativeApi: ProductsNativeApiProtocol?
    private var containerLoaded = false
    private var scriptLoaded = false

    private var renderContinuations: [String: AsyncThrowingStream<String, Error>.Continuation] = [:]

    public init(
        productUrl: URL,
        scriptsFactory: ChatScriptsMaking,
        engineFactory: @Sendable @escaping () -> JSEngineProtocol,
        logger: SDKLoggerProtocol
    ) {
        self.productUrl = productUrl
        self.scriptsFactory = scriptsFactory
        self.engineFactory = engineFactory
        self.logger = logger
    }

    // MARK: - ProductsScriptExecutorProtocol

    public func initializeBot(nativeApi: ProductsNativeApiProtocol) async throws {
        self.nativeApi = nativeApi

        _ = try await getOrCreateEngine()
        try await ensureScriptLoaded()
    }

    public func onBotStarted() async throws {
        let jsEngine = try requireEngine()

        let script = """
        (function() {
            if (typeof onBotStarted === 'function') {
                onBotStarted();
            }
        })();
        """

        try await jsEngine.evaluate(script)
    }

    public func onUserMessage(text: String, roomId: String?) async throws {
        let jsEngine = try requireEngine()
        let escapedText = text.jsEscaped
        let escapedRoomId = (roomId ?? "").jsEscaped

        // Route through container's dispatchUserMessage global
        // which delivers MessagePosted events via the host-api protocol
        try await jsEngine.evaluate("dispatchUserMessage('\(escapedRoomId)', '\(escapedText)')")
    }

    public func renderMessage(
        messageId: String,
        messageType: String,
        messageData: Data
    ) -> AsyncThrowingStream<String, Error> {
        let (stream, continuation) = AsyncThrowingStream.makeStream(of: String.self)

        renderContinuations[messageId] = continuation

        Task { [weak self] in
            guard let self else {
                continuation.finish(throwing: ScriptExecutorError.deallocated)
                return
            }

            do {
                try await ensureScriptLoaded()

                let jsScript = await buildRenderingJS(
                    messageType: messageType,
                    messageData: messageData,
                    messageId: messageId
                )
                try await getOrCreateEngine().evaluate(jsScript)
            } catch {
                continuation.finish(throwing: error)
                await removeContinuation(for: messageId)
            }
        }

        continuation.onTermination = { [weak self] _ in
            Task { await self?.removeContinuation(for: messageId) }
        }

        return stream
    }

    public func dispatchEvent(roomId: String?, messageId: String, actionId: String, payload: String?) async {
        do {
            let jsEngine = try requireEngine()

            let escapedRoomId = (roomId ?? "").jsEscaped
            let escapedMessageId = messageId.jsEscaped
            let escapedActionId = actionId.jsEscaped
            let escapedPayload = try payload?.scaleEncoded().toHex() ?? "undefined"

            try await jsEngine.evaluate(
                "dispatchChatAction('\(escapedRoomId)', '\(escapedMessageId)', '\(escapedActionId)', '\(escapedPayload)')"
            )
        } catch {
            logger.error("Failed to dispatch event \(actionId): \(error)")
        }
    }

    public func dispose() async {
        engineMonitor?.stop()
        engineMonitor = nil

        await containerBridge?.dispose()
        containerBridge = nil

        await moduleBridge?.dispose()
        moduleBridge = nil

        await engine?.destroy()
        engine = nil

        scriptLoaded = false
        containerLoaded = false

        for (_, continuation) in renderContinuations {
            continuation.finish()
        }
        renderContinuations.removeAll()

        logger.debug("Disposed script executor for: \(productUrl)")
    }
}

private extension ProductsScriptExecutor {
    func getOrCreateEngine() async throws -> JSEngineProtocol {
        if let engine { return engine }

        let newEngine = engineFactory()
        try await newEngine.initialize()

        let currentState = await newEngine.getState()
        guard currentState == .ready else {
            throw ScriptExecutorError.engineInitFailed
        }

        let monitor = JSEngineMonitor(
            engine: newEngine,
            pauseEvent: .willResignActive,
            resumeEvent: .didBecomeActive
        )
        monitor.start()
        engineMonitor = monitor

        engine = newEngine
        return newEngine
    }

    func requireEngine() throws -> JSEngineProtocol {
        guard let engine else {
            throw ScriptExecutorError.engineNotInitialized
        }
        return engine
    }

    // MARK: - Script Loading

    func ensureContainerLoaded() async throws {
        guard !containerLoaded else { return }

        let jsEngine = try await getOrCreateEngine()

        let bridge = ContainerBridge(engine: jsEngine, logger: logger)

        guard let nativeApi else {
            throw ScriptExecutorError.engineNotInitialized
        }

        await bridge.registerHostApiHandlers(
            nativeApi: nativeApi,
            onRenderWidget: { [weak self] messageId, scaleHex in
                Task { await self?.handleRenderWidget(messageId: messageId, scaleHex: scaleHex) }
            }
        )
        await bridge.install()

        containerBridge = bridge

        let modBridge = JSESModuleBridge(engine: jsEngine)
        await modBridge.install()
        moduleBridge = modBridge

        for script in try scriptsFactory.makeScripts() {
            try await jsEngine.evaluate(script)
        }

        containerLoaded = true
    }

    func ensureScriptLoaded() async throws {
        guard !scriptLoaded else { return }

        try await ensureContainerLoaded()

        guard let moduleBridge else {
            throw ScriptExecutorError.engineNotInitialized
        }

        try await moduleBridge.executeScript(url: productUrl)

        scriptLoaded = true
        logger.debug("Loaded script from: \(productUrl)")
    }

    // MARK: - Render Widget Callback

    func handleRenderWidget(messageId: String, scaleHex: String) {
        guard let continuation = renderContinuations[messageId] else {
            logger.warning("Render update for unknown messageId: \(messageId)")
            return
        }
        continuation.yield(scaleHex)
    }

    func removeContinuation(for messageId: String) {
        renderContinuations[messageId] = nil
    }

    // MARK: - JS Helpers

    func buildRenderingJS(
        messageType: String,
        messageData: Data,
        messageId: String
    ) -> String {
        let hexData = messageData.toHex(includePrefix: true)
        let escapedMessageId = messageId.jsEscapedDoubleQuote
        let escapedMessageType = messageType.jsEscapedDoubleQuote

        return """
        (function() {
            try {
                if (typeof window.renderMessage === 'function') {
                    window.renderMessage("\(escapedMessageType)", "\(hexData)", "\(escapedMessageId)");
                }
            } catch (e) {
                console.error('renderMessage error:', e);
            }
        })();
        """
    }
}

// MARK: - Errors

public enum ScriptExecutorError: Error, LocalizedError {
    case engineNotInitialized
    case engineInitFailed
    case scriptNotFound(productId: String)
    case deallocated

    public var errorDescription: String? {
        switch self {
        case .engineNotInitialized:
            "Script engine not initialized"
        case .engineInitFailed:
            "Failed to initialize JS engine"
        case let .scriptNotFound(productId):
            "Script not found for product: \(productId)"
        case .deallocated:
            "Script executor was deallocated"
        }
    }
}
