import Foundation
import Products
import SubstrateSdk
import SubstrateSdkExt
import Testing

@testable import polkadot_app

/// Every product-originated request opens with its caller: the product id, then the
/// executable kind. The fixtures are the bytes the TrUAPI core's SSO wire tests pin.
@Suite("Product Request SCALE Coding Tests")
struct ProductRequestScaleCodingTests {
    static let playground = PolkadotHostRemoteMessage.ProductCaller(
        productId: "truapi-playground.dot",
        executionKind: .app
    )

    @Test("A resource allocation request names its caller")
    func resourceAllocationRequestNamesItsCaller() throws {
        let message = try decode(
            "286d2d7265736f757263650005547472756170692d706c617967726f756e642e646f74"
                + "001000010200090000000301"
        )

        guard case let .resourceAllocationRequest(request) = message.latestContent() else {
            Issue.record("Expected resourceAllocationRequest, got \(String(describing: message.latestContent()))")
            return
        }

        #expect(request.caller == Self.playground)
        #expect(request.payload.resources.count == 4)
    }

    @Test("A transaction request names its caller before its payload")
    func createTransactionRequestNamesItsCaller() throws {
        let message = try decode(
            "306d2d70726f647563742d74780007547472756170692d706c617967726f756e642e646f74"
                + "0000547472756170692d706c617967726f756e642e646f740000000000202122232425"
                + "262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f0800000428436865636b"
                + "4e6f6e6365040108020300"
        )

        guard case let .createTransactionRequest(request) = message.latestContent() else {
            Issue.record("Expected createTransactionRequest, got \(String(describing: message.latestContent()))")
            return
        }

        #expect(request.caller == Self.playground)
    }

    @Test("A caller round-trips its executable kind")
    func callerRoundTripsItsExecutableKind() throws {
        let caller = PolkadotHostRemoteMessage.ProductCaller(productId: "game.dot", executionKind: .worker)
        let encoder = ScaleEncoder()
        try caller.encode(scaleEncoder: encoder)

        #expect(encoder.encode() == Data([0x20]) + Data("game.dot".utf8) + Data([2]))
        let decoded = try PolkadotHostRemoteMessage.ProductCaller(scaleDecoder: ScaleDecoder(data: encoder.encode()))
        #expect(decoded == caller)
    }

    private func decode(_ hex: String) throws -> PolkadotHostRemoteMessage {
        try PolkadotHostRemoteMessage(scaleDecoder: ScaleDecoder(data: hex.fromHex()))
    }
}
