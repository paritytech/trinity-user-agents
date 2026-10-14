import Foundation

// MARK: - Engine State

public enum JSEngineState: Equatable {
    case initializing
    case ready
    case error(String)
    case destroyed
}

// MARK: - Native Handler

/// Handler for native functions called from JavaScript.
/// Receives arguments as a JSON string.
public typealias JSNativeHandler = (_ args: String) async throws -> Void

// MARK: - Engine Protocol

public protocol JSEngineProtocol: AnyObject {
    func getState() async -> JSEngineState

    /// Initialize the engine, optionally injecting scripts before the page loads.
    /// Transitions state from `.initializing` to `.ready`.
    func initialize(with scripts: [JSEngineScript]) async throws

    /// Evaluate a JavaScript string in the engine context.
    @discardableResult
    func evaluate(_ script: String) async throws -> Any?

    /// Register a native function that can be called from JavaScript.
    func registerFunction(name: String, handler: @escaping JSNativeHandler) async

    /// Dispatch an event to a registered JS callback via `NativeBridge._dispatchEvent`.
    func dispatchEvent(actionId: String, payload: String) async throws

    /// Tear down the engine and release all resources.
    func destroy() async

    /// Register a handler that decides whether to grant media capture permissions.
    func registerJSDeviceCapabilityHandler(_ handler: @escaping JSDeviceCapabilityHandler) async
}

public extension JSEngineProtocol {
    func initialize() async throws {
        try await initialize(with: [])
    }
}
