import Products

/// An engine whose page never comes up, which is what a worker archive that
/// cannot be loaded looks like from here.
final class FailingJSEngine: JSEngineProtocol, @unchecked Sendable {
    func getState() async -> JSEngineState { .error("no page") }
    func initialize(with _: [JSEngineScript]) async throws { throw ScriptExecutorError.engineInitFailed }
    func evaluate(_: String) async throws -> Any? { nil }
    func registerFunction(name _: String, handler _: @escaping JSNativeHandler) async {}
    func dispatchEvent(actionId _: String, payload _: String) async throws {}
    func destroy() async {}
    func registerJSDeviceCapabilityHandler(_: @escaping JSDeviceCapabilityHandler) async {}
}
