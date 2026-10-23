import Foundation
import Testing
import SDKLogger
@testable import Products

@Suite("ContainerBridge pending request watchdog")
struct ContainerBridgeTests {
    @Test("a request still unanswered after the delay is logged as a warning")
    func slowRequestIsLogged() async throws {
        let engine = RecordingJSEngine()
        let logger = RecordingLogger()
        let bridge = ContainerBridge(engine: engine, logger: logger, pendingRequestWarningDelay: .milliseconds(20))
        let gate = AsyncGate()

        await bridge.registerRequestHandler(method: "slow") { _ in
            await gate.wait()
            return .null
        }
        await bridge.install()

        let request = Task {
            try await engine.invokeNative("__container__", args: #"{"type":"request","id":"r1","method":"slow"}"#)
        }
        try await Task.sleep(for: .milliseconds(200))
        await gate.open()
        try await request.value

        #expect(logger.warnings.contains { $0.contains("'slow' request r1 still pending") })
    }

    @Test("a request answered in time is not logged")
    func fastRequestIsNotLogged() async throws {
        let engine = RecordingJSEngine()
        let logger = RecordingLogger()
        let bridge = ContainerBridge(engine: engine, logger: logger, pendingRequestWarningDelay: .milliseconds(200))

        await bridge.registerRequestHandler(method: "fast") { _ in .null }
        await bridge.install()

        try await engine.invokeNative("__container__", args: #"{"type":"request","id":"r1","method":"fast"}"#)
        try await Task.sleep(for: .milliseconds(300))

        #expect(logger.warnings.isEmpty)
    }
}

private actor AsyncGate {
    private var continuations: [CheckedContinuation<Void, Never>] = []
    private var isOpen = false

    func wait() async {
        guard !isOpen else { return }
        await withCheckedContinuation { continuations.append($0) }
    }

    func open() {
        isOpen = true
        continuations.forEach { $0.resume() }
        continuations.removeAll()
    }
}

private final class RecordingJSEngine: JSEngineProtocol, @unchecked Sendable {
    private var handlers: [String: JSNativeHandler] = [:]

    func getState() async -> JSEngineState { .ready }
    func initialize(with _: [JSEngineScript]) async throws {}
    func evaluate(_: String) async throws -> Any? { nil }
    func registerFunction(name: String, handler: @escaping JSNativeHandler) async { handlers[name] = handler }
    func dispatchEvent(actionId _: String, payload _: String) async throws {}
    func destroy() async {}
    func registerJSDeviceCapabilityHandler(_: @escaping JSDeviceCapabilityHandler) async {}

    func invokeNative(_ name: String, args: String) async throws {
        guard let handler = handlers[name] else { throw MissingFunction() }
        try await handler(args)
    }

    struct MissingFunction: Error {}
}

private final class RecordingLogger: SDKLoggerProtocol, @unchecked Sendable {
    private let lock = NSLock()
    private var storedWarnings: [String] = []

    var warnings: [String] { lock.withLock { storedWarnings } }

    func verbose(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func debug(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func info(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func error(message _: () -> String, file _: String, function _: String, line _: Int) {}

    func warning(message: () -> String, file _: String, function _: String, line _: Int) {
        let text = message()
        lock.withLock { storedWarnings.append(text) }
    }
}
