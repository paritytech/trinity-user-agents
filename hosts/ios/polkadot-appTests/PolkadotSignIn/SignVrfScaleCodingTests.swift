import Foundation
import KeyDerivation
import Products
import SubstrateSdk
import Testing

@testable import polkadot_app

@Suite("SignVrf SCALE Coding Tests")
struct SignVrfScaleCodingTests {
    let request = PolkadotHostRemoteMessage.SignVrfRequest(
        callingProductId: "caller.product",
        payload: SignVrfPayload(
            account: ProductAccountId(productId: "target.product", derivationIndex: .index(3)),
            transcriptLabel: Data("test:label".utf8),
            items: [
                VrfTranscriptItem(label: Data("domain".utf8), value: Data.random(of: 16)!),
                VrfTranscriptItem(label: Data("signer".utf8), value: Data.random(of: 32)!)
            ]
        )
    )

    let signature = PolkadotHostRemoteMessage.VrfSignatureWire(
        preOutput: Data.random(of: 32)!,
        proof: Data.random(of: 64)!
    )

    // Wire enum indices are the cross-host contract with the Android implementation.
    @Test("Content indices are 14 for request and 15 for response")
    func contentIndicesMatchWireContract() throws {
        let requestContent = PolkadotHostRemoteMessage.ContentV1.signVrfRequest(request)
        #expect(try encoded(requestContent).first == 14)

        let responseContent = PolkadotHostRemoteMessage.ContentV1.signVrfResponse(
            requestMessageId: "id",
            result: .success(signature)
        )
        #expect(try encoded(responseContent).first == 15)
    }

    @Test("Error indices are notConnected 0, rejected 1, unknown 2")
    func errorIndicesMatchWireContract() throws {
        #expect(try encoded(PolkadotHostRemoteMessage.SignVrfWireError.notConnected) == Data([0]))
        #expect(try encoded(PolkadotHostRemoteMessage.SignVrfWireError.rejected) == Data([1]))

        let unknown = try encoded(PolkadotHostRemoteMessage.SignVrfWireError.unknown("boom"))
        #expect(unknown.first == 2)
    }

    @Test("Signature encodes as fixed-size arrays, no length prefixes")
    func signatureLayoutIsFixedSize() throws {
        // Spec wire shape (RFC-0023): pre_output is `[u8; 32]`, proof is `[u8; 64]` —
        // raw bytes, no compact length prefixes.
        let encoded = try encoded(signature)

        #expect(encoded == signature.preOutput + signature.proof)
        #expect(encoded.count == 96)
    }

    @Test("Truncated signature bytes fail to decode")
    func truncatedSignatureFailsDecoding() throws {
        let truncated = try Data.randomOrError(of: 95)

        #expect(throws: (any Error).self) {
            _ = try PolkadotHostRemoteMessage.VrfSignatureWire(
                scaleDecoder: ScaleDecoder(data: truncated)
            )
        }
    }

    @Test("Signature with wrong field lengths fails to encode")
    func wrongLengthSignatureFailsEncoding() throws {
        let malformed = try PolkadotHostRemoteMessage.VrfSignatureWire(
            preOutput: Data.randomOrError(of: 31),
            proof: Data.randomOrError(of: 64)
        )

        #expect(throws: (any Error).self) {
            _ = try encoded(malformed)
        }
    }

    @Test("Request round-trips preserving caller, account, and items")
    func requestRoundTrips() throws {
        let content = PolkadotHostRemoteMessage.ContentV1.signVrfRequest(request)

        let decoded = try PolkadotHostRemoteMessage.ContentV1(
            scaleDecoder: ScaleDecoder(data: encoded(content))
        )

        #expect(decoded == content)
    }

    @Test("Response round-trips for success and failure results")
    func responseRoundTrips() throws {
        let results: [PolkadotHostRemoteMessage.SignVrfHostResult] = [
            .success(signature),
            .failure(.rejected),
            .failure(.unknown("transcript too large"))
        ]

        for result in results {
            let content = PolkadotHostRemoteMessage.ContentV1.signVrfResponse(
                requestMessageId: "request-id",
                result: result
            )

            let decoded = try PolkadotHostRemoteMessage.ContentV1(
                scaleDecoder: ScaleDecoder(data: encoded(content))
            )

            #expect(decoded == content)
        }
    }

    private func encoded(_ value: some ScaleEncodable) throws -> Data {
        let encoder = ScaleEncoder()
        try value.encode(scaleEncoder: encoder)
        return encoder.encode()
    }
}
