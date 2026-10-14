import Foundation
import Products

final class MockJSEngine: JSEngineProtocol, @unchecked Sendable {
    var evaluatedScripts: [String] = []
    private(set) var initializedScripts: [JSEngineScript] = []
    private(set) var destroyCallCount = 0
    private(set) var deviceCapabilityHandler: JSDeviceCapabilityHandler?
    private(set) var mediaHandlerWasInstalledAtInitialization = false
    var onInitialize: (@Sendable () async -> Void)?
    private var handlers: [String: JSNativeHandler] = [:]

    func getState() async -> JSEngineState { .ready }

    func initialize(with scripts: [JSEngineScript]) async throws {
        mediaHandlerWasInstalledAtInitialization = deviceCapabilityHandler != nil
        initializedScripts = scripts
        await onInitialize?()
    }

    func evaluate(_ script: String) async throws -> Any? {
        evaluatedScripts.append(script)

        // JSESModuleBridge awaits __module_complete__ for the injected module
        // tag; resolve it inline so executeScript does not hang.
        if let range = script.range(of: "__module_complete__.postMessage('") {
            let rest = script[range.upperBound...]
            if let end = rest.firstIndex(of: "'"), let handler = handlers["__module_complete__"] {
                try await handler(String(rest[..<end]))
            }
        }

        return nil
    }

    func registerFunction(name: String, handler: @escaping JSNativeHandler) async {
        handlers[name] = handler
    }

    /// Calls a native function the way the page would, so a test can drive a bridge end to end.
    func invokeNative(_ name: String, args: String) async throws {
        guard let handler = handlers[name] else {
            throw MissingNativeFunction(name: name)
        }
        try await handler(args)
    }

    struct MissingNativeFunction: Error {
        let name: String
    }

    func dispatchEvent(actionId _: String, payload _: String) async throws {}

    func destroy() async {
        destroyCallCount += 1
    }

    func registerJSDeviceCapabilityHandler(_ handler: @escaping JSDeviceCapabilityHandler) async {
        deviceCapabilityHandler = handler
    }
}
