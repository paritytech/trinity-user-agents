import Products
import StructuredConcurrencyTestSupport

/// An engine that hangs and then fails, which is what a worker whose page is
/// still loading when its execution is closed underneath it looks like.
final class SlowFailingJSEngine: JSEngineProtocol, @unchecked Sendable {
    private let delay: Duration
    private let clock: ManualClock

    init(after delay: Duration, clock: ManualClock) {
        self.delay = delay
        self.clock = clock
    }

    func getState() async -> JSEngineState { .error("no page") }
    func initialize(with _: [JSEngineScript]) async throws {
        try? await clock.sleep(for: delay)
        throw ScriptExecutorError.engineInitFailed
    }

    func evaluate(_: String) async throws -> Any? { nil }
    func registerFunction(name _: String, handler _: @escaping JSNativeHandler) async {}
    func dispatchEvent(actionId _: String, payload _: String) async throws {}
    func destroy() async {}
    func registerJSDeviceCapabilityHandler(_: @escaping JSDeviceCapabilityHandler) async {}
}
