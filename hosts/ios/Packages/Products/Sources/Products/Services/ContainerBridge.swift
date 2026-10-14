import Foundation
import SDKLogger
import SubstrateSdk
import AsyncExtensions

// MARK: - Handler Types

/// Handler for one-shot request → response calls.
public typealias ContainerRequestHandler = (_ params: JSON) async throws -> JSON

/// Handler for subscriptions. Returns an async stream of updates.
/// The bridge sends each emitted value as an `{"update": ...}` callback,
/// and sends `{"complete": true}` when the stream finishes.
public typealias ContainerSubscriptionHandler = (_ params: JSON) async throws -> AnyAsyncSequence<JSON>

/// Errors carrying a wire code serialize as `{"error": {"code": ..., "message": ...}}`
/// so product scripts can reconstruct typed errors. Other errors keep the legacy string form.
public protocol HostCallCodedError: Error {
    var code: String { get }
    var message: String { get }
}

// MARK: - Container Bridge

/// Routes typed request/response and subscription messages between JS and native host API handlers.
///
/// Installs itself on the engine via `registerFunction("__container__", ...)`.
/// JS sends messages via `window.webkit.messageHandlers.__container__.postMessage(json)`.
/// The bridge parses them, routes to registered handlers, and responds via
/// `window.__container_callback__(id, responseJSON)` evaluated in the engine.
public actor ContainerBridge {
    private let engine: JSEngineProtocol
    private nonisolated let logger: SDKLoggerProtocol

    private var requestHandlers: [String: ContainerRequestHandler] = [:]
    private var subscriptionHandlers: [String: ContainerSubscriptionHandler] = [:]
    private var activeSubscriptions: [String: Task<Void, Never>] = [:]
    private var jsDeviceCapabilityHandler: JSDeviceCapabilityHandler?

    public init(engine: JSEngineProtocol, logger: SDKLoggerProtocol) {
        self.engine = engine
        self.logger = logger
    }

    // MARK: - Handler Registration

    func registerRequestHandler(method: String, handler: @escaping ContainerRequestHandler) {
        requestHandlers[method] = handler
    }

    func registerSubscriptionHandler(method: String, handler: @escaping ContainerSubscriptionHandler) {
        subscriptionHandlers[method] = handler
    }

    func registerJSDeviceCapabilityHandler(_ handler: @escaping JSDeviceCapabilityHandler) {
        jsDeviceCapabilityHandler = handler
    }

    // MARK: - Install / Dispose

    /// Register the `__container__` handler on the engine.
    /// Must be called before evaluating container.js.
    public func install() async {
        await engine.registerFunction(name: "__container__") { [weak self] args in
            await self?.handleMessage(args)
        }

        if let handler = jsDeviceCapabilityHandler {
            await engine.registerJSDeviceCapabilityHandler(handler)
        }
    }

    public func dispose() {
        for (_, task) in activeSubscriptions {
            task.cancel()
        }
        activeSubscriptions.removeAll()
    }

    // MARK: - Message Routing

    private func handleMessage(_ raw: String) async {
        let data = Data(raw.utf8)

        guard
            let json = try? JSONDecoder().decode(JSON.self, from: data),
            let type = json["type"]?.stringValue,
            let id = json["id"]?.stringValue else {
            logger.warning("Invalid message format")
            return
        }

        switch type {
        case "request":
            await handleRequest(id: id, json: json)
        case "subscribe":
            await handleSubscribe(id: id, json: json)
        case "unsubscribe":
            await handleUnsubscribe(id: id)
        default:
            logger.warning("Unknown message type '\(type)'")
        }
    }

    // MARK: - Request Handling

    private func handleRequest(id: String, json: JSON) async {
        logger.debug("Request: \(json)")

        guard let method = json["method"]?.stringValue else {
            await sendError(id: id, message: "Missing method")
            logger.error("Error: Missing method")
            return
        }

        guard let handler = requestHandlers[method] else {
            await sendError(id: id, message: "Unknown method '\(method)'")
            logger.error("Error: unknown '\(method)'")
            return
        }

        let params = json["params"] ?? .null

        do {
            let result = try await handler(params)

            logger.debug("Response: \(result)")

            await sendValue(id: id, value: result)
        } catch {
            await sendError(id: id, error: error)
            logger.error("Error: \(error)")
        }
    }

    // MARK: - Subscription Handling

    private func handleSubscribe(id: String, json: JSON) async {
        logger.debug("Subscribe: \(json)")

        guard let method = json["method"]?.stringValue else {
            await sendError(id: id, message: "Missing method")
            logger.error("Error: Missing method")
            return
        }

        guard let handler = subscriptionHandlers[method] else {
            await sendError(id: id, message: "Unknown subscription method '\(method)'")
            logger.error("Error: unknown '\(method)'")
            return
        }

        let params = json["params"] ?? .null

        do {
            let stream = try await handler(params)

            let task = Task { [weak self] in
                do {
                    for try await update in stream {
                        guard !Task.isCancelled else { break }
                        self?.logger.debug("Sending update: \(update) with id \(id)")
                        await self?.sendUpdate(id: id, value: update)
                        self?.logger.debug("Sent update to id \(id)")
                    }

                    if !Task.isCancelled {
                        await self?.sendComplete(id: id)
                        self?.logger.debug("Send complete with id \(id)")
                    }

                    await self?.handleSubscriptionClear(id: id)
                } catch {
                    guard !Task.isCancelled else { return }

                    self?.logger.error("Subscription \(id) failed mid-stream: \(error)")
                    await self?.sendError(id: id, error: error)
                    await self?.handleSubscriptionClear(id: id)
                }
            }

            activeSubscriptions[id] = task
        } catch {
            await sendError(id: id, error: error)
            logger.error("Subscribe error: \(error)")
        }
    }

    private func handleUnsubscribe(id: String) async {
        activeSubscriptions[id]?.cancel()
        activeSubscriptions[id] = nil
    }

    private func handleSubscriptionClear(id: String) {
        activeSubscriptions[id] = nil
    }

    // MARK: - Response Sending

    private func sendValue(id: String, value: JSON) async {
        let response = JSON.dictionaryValue(["value": value])
        await sendCallback(id: id, responseJSON: response)
    }

    private func sendError(id: String, message: String) async {
        let response = JSON.dictionaryValue(["error": JSON.stringValue(message)])
        await sendCallback(id: id, responseJSON: response)
    }

    private func sendError(id: String, error: Error) async {
        guard let codedError = error as? HostCallCodedError else {
            await sendError(id: id, message: error.localizedDescription)
            return
        }

        let payload = JSON.dictionaryValue([
            "code": JSON.stringValue(codedError.code),
            "message": JSON.stringValue(codedError.message)
        ])

        await sendCallback(id: id, responseJSON: JSON.dictionaryValue(["error": payload]))
    }

    private func sendUpdate(id: String, value: JSON) async {
        let response = JSON.dictionaryValue(["update": value])
        await sendCallback(id: id, responseJSON: response)
    }

    private func sendComplete(id: String) async {
        let response = JSON.dictionaryValue(["complete": JSON.boolValue(true)])
        await sendCallback(id: id, responseJSON: response)
    }

    private func sendCallback(id: String, responseJSON: JSON) async {
        do {
            let escapedId = id.jsEscaped
            let escapedResponse = try responseJSON.encodedString().jsEscaped
            let script = "window.__container_callback__('\(escapedId)', '\(escapedResponse)')"

            try await engine.evaluate(script)
        } catch {
            logger.error("Failed to send callback for \(id): \(error)")
        }
    }
}
