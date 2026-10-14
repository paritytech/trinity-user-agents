import Foundation
import Individuality
import MessageExchangeKit
import Products
import SubstrateSdk
import Testing
import UIKitExt

@testable import polkadot_app

/// Wire pins for the `ApProductSubtree` message pair.
/// iOS defines these indices and layouts — they are the cross-platform contract.
@Suite("ProductSubtree Message Tests")
struct ProductSubtreeMessageTests {
    @Test("Content indices are 16 for request and 17 for response")
    func contentIndicesMatchWireContract() throws {
        let request = PolkadotHostRemoteMessage.ContentV1.productSubtreeRequest(
            .init(productId: "browse.dot")
        )
        let response = PolkadotHostRemoteMessage.ContentV1.productSubtreeResponse(
            requestMessageId: "id",
            result: .success(.init(productPublicKey: Data.random(of: 32)!))
        )

        #expect(try encoded(request).first == 16)
        #expect(try encoded(response).first == 17)
    }

    @Test("Response key encodes as a fixed 32-byte array, no length prefix")
    func responseKeyLayoutIsFixedSize() throws {
        let key = Data.random(of: 32)!

        let encoded = try encoded(PolkadotHostRemoteMessage.ProductSubtreeKey(productPublicKey: key))

        #expect(encoded == key)
    }

    @Test("Content bytes match the cross-platform vectors")
    func contentBytesMatchCrossPlatformVectors() throws {
        // index 16 ++ compact-len utf8 productId | index 17 ++ requestMessageId ++ Ok ++ [u8; 32].
        let request = PolkadotHostRemoteMessage.ContentV1.productSubtreeRequest(
            .init(productId: "browse.dot")
        )
        let response = PolkadotHostRemoteMessage.ContentV1.productSubtreeResponse(
            requestMessageId: "request",
            result: .success(.init(productPublicKey: Data(repeating: 0xAB, count: 32)))
        )

        #expect(try encoded(request) == Data(hexString: "0x102862726f7773652e646f74"))
        #expect(try encoded(response) == Data(
            hexString: "0x111c7265717565737400" + String(repeating: "ab", count: 32)
        ))
    }

    @Test("Request and response round-trip")
    func roundTrips() throws {
        let contents: [PolkadotHostRemoteMessage.ContentV1] = [
            .productSubtreeRequest(.init(productId: "browse.dot")),
            .productSubtreeResponse(
                requestMessageId: "request-1",
                result: .success(.init(productPublicKey: Data.random(of: 32)!))
            ),
            .productSubtreeResponse(
                requestMessageId: "request-2",
                result: .failure("invalid product id")
            )
        ]

        for content in contents {
            let decoded = try PolkadotHostRemoteMessage.ContentV1(
                scaleDecoder: ScaleDecoder(data: encoded(content))
            )
            #expect(decoded == content)
        }
    }

    @Test("Wrong-length public key fails to encode")
    func wrongLengthKeyFailsEncoding() throws {
        let malformed = try PolkadotHostRemoteMessage.ProductSubtreeKey(
            productPublicKey: Data.randomOrError(of: 31)
        )

        #expect(throws: (any Error).self) {
            _ = try encoded(malformed)
        }
    }

    @Test("Handler responds with the subtree key without any consent prompt")
    func handlerIsConsentFree() async throws {
        let accountManager = MockProductsAccountManager()
        let sender = MockHostMessageSender()
        let handler = SSOProductSubtreeRequestHandler(
            accountManager: accountManager,
            messageSender: sender,
            logger: MockLogger()
        )

        let message = PolkadotHostRemoteMessage(
            messageId: "request-1",
            versionedContent: .v1(.productSubtreeRequest(.init(productId: "browse.dot")))
        )

        await handler.handle(message: message, from: makeHost())

        guard
            case let .productSubtreeResponse(requestMessageId, .success(key))? =
            sender.postedMessages.first?.latestContent()
        else {
            Issue.record("Expected successful productSubtreeResponse")
            return
        }

        #expect(requestMessageId == "request-1")
        #expect(key.productPublicKey == accountManager.subtreePublicKey)
        #expect(accountManager.requestedProductIds == ["browse.dot"])
    }

    @Test("Handler reports a failure result when derivation throws")
    func handlerReportsFailure() async throws {
        let accountManager = MockProductsAccountManager()
        accountManager.subtreeError = MockDerivationError.invalidProductId
        let sender = MockHostMessageSender()
        let handler = SSOProductSubtreeRequestHandler(
            accountManager: accountManager,
            messageSender: sender,
            logger: MockLogger()
        )

        let message = PolkadotHostRemoteMessage(
            messageId: "request-2",
            versionedContent: .v1(.productSubtreeRequest(.init(productId: "bad/id")))
        )

        await handler.handle(message: message, from: makeHost())

        guard
            case .productSubtreeResponse(_, .failure)? =
            sender.postedMessages.first?.latestContent()
        else {
            Issue.record("Expected failed productSubtreeResponse")
            return
        }
    }

    private func makeHost() -> PolkadotSignInHost {
        PolkadotSignInHost(
            accountId: Data.random(of: 32)!,
            publicKey: Data.random(of: 32)!,
            name: "TestHost",
            iconUrl: nil
        )
    }

    private func encoded(_ value: some ScaleEncodable) throws -> Data {
        let encoder = ScaleEncoder()
        try value.encode(scaleEncoder: encoder)
        return encoder.encode()
    }
}

// MARK: - Test Errors

private enum MockDerivationError: Error {
    case invalidProductId
}
