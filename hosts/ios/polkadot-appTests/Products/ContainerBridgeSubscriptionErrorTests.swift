import AsyncExtensions
import Foundation
import Products
import Testing
@testable import polkadot_app

/// A subscription that fails must reach the page as a coded error — on subscribe and mid-stream —
/// so `index.ts` can rebuild the typed `PaymentTopUpStatusErr` instead of falling back to Unknown.
struct ContainerBridgeSubscriptionErrorTests {
    private static let subscribeMessage = """
    {"type":"subscribe","id":"s1","method":"paymentTopUpStatusSubscribe","params":{"id":"0x01"}}
    """

    @Test func subscribeFailureIsSentAsACodedError() async throws {
        let engine = MockJSEngine()
        let nativeApi = StubProductsNativeApi()
        nativeApi.paymentTopUpStatusResult = .failure(HostPaymentTopUpError.notFound("01"))
        let bridge = await makeBridge(engine: engine, nativeApi: nativeApi)
        _ = bridge

        try await engine.invokeNative("__container__", args: Self.subscribeMessage)

        let payload = try #require(try callbackPayloads(engine, id: "s1").first)
        #expect(errorCode(in: payload) == "NotFound")
    }

    @Test(.timeLimit(.minutes(1)))
    func midStreamFailureIsSentAsACodedErrorAfterTheUpdates() async throws {
        let engine = MockJSEngine()
        let nativeApi = StubProductsNativeApi()
        nativeApi.paymentTopUpStatusResult = .success(
            AsyncThrowingStream<HostPaymentTopUpStatus, Error> { continuation in
                continuation.yield(.detecting)
                continuation.finish(throwing: HostPaymentTopUpError.unknown(reason: "ledger gone"))
            }.eraseToAnyAsyncSequence()
        )
        let bridge = await makeBridge(engine: engine, nativeApi: nativeApi)
        _ = bridge

        try await engine.invokeNative("__container__", args: Self.subscribeMessage)

        let payloads = try await waitForPayloads(engine, id: "s1", count: 2)
        #expect(payloads[0]["update"] != nil)
        #expect(errorCode(in: payloads[1]) == "Unknown")
    }

    private func makeBridge(engine: MockJSEngine, nativeApi: StubProductsNativeApi) async -> ContainerBridge {
        let bridge = ContainerBridge(engine: engine, logger: StubLogger())
        await bridge.registerHostApiHandlers(nativeApi: nativeApi) { _, _ in }
        await bridge.install()
        return bridge
    }

    private func waitForPayloads(
        _ engine: MockJSEngine,
        id: String,
        count: Int
    ) async throws -> [[String: Any]] {
        let deadline = ContinuousClock.now.advanced(by: .seconds(10))
        while ContinuousClock.now < deadline {
            let payloads = try callbackPayloads(engine, id: id)
            if payloads.count >= count { return payloads }
            try await Task.sleep(for: .milliseconds(10))
        }
        return try callbackPayloads(engine, id: id)
    }

    /// The JSON objects handed to `__container_callback__` for `id`, in order.
    private func callbackPayloads(_ engine: MockJSEngine, id: String) throws -> [[String: Any]] {
        let prefix = "window.__container_callback__('\(id)', '"
        return try engine.evaluatedScripts.compactMap { script -> [String: Any]? in
            guard script.hasPrefix(prefix), script.hasSuffix("')") else { return nil }
            let escaped = script.dropFirst(prefix.count).dropLast(2)
            let json = String(escaped)
                .replacingOccurrences(of: "\\'", with: "'")
                .replacingOccurrences(of: "\\\\", with: "\\")
            let object = try JSONSerialization.jsonObject(with: Data(json.utf8))
            return object as? [String: Any]
        }
    }

    private func errorCode(in payload: [String: Any]) -> String? {
        (payload["error"] as? [String: Any])?["code"] as? String
    }
}
