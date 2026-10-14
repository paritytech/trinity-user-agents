import Foundation
import Products
import SubstrateSdk
import Testing

@testable import polkadot_app

/// Wire-compatibility assertions only: ContentV1 enum indices and JSON shapes are
/// cross-platform contracts (Android `SsoMessageContent`, JS SDK).
@Suite("APPersonhood Coding Tests")
struct APPersonhoodCodingTests {
    @Test("Alias request encodes at ContentV1 index 3 with the RFC field order")
    func aliasRequestMessageRoundtrip() throws {
        let request = PolkadotHostRemoteMessage.AliasRequest(
            callingProductId: "caller.product",
            context: ProductProofContext(productId: "target.product", suffix: .index(0)),
            ring: RingLocation(chainId: Data.random(of: 32)!, junctions: [.palletInstance(1)])
        )

        let content = PolkadotHostRemoteMessage.ContentV1.aliasRequest(request)
        let decoded = try roundtripContent(content, expectedIndex: 3)

        guard case let .aliasRequest(decodedRequest) = decoded else {
            Issue.record("Expected aliasRequest, got \(decoded)")
            return
        }

        #expect(decodedRequest.callingProductId == request.callingProductId)
        #expect(decodedRequest.context == request.context)
        #expect(decodedRequest.ring == request.ring)
    }

    @Test("Alias response encodes at ContentV1 index 4")
    func aliasResponseMessageRoundtrip() throws {
        let alias = ContextualAlias(context: Data.random(of: 32)!, alias: Data.random(of: 33)!)
        let content = PolkadotHostRemoteMessage.ContentV1.aliasResponse(
            requestMessageId: "message-1",
            result: .success(alias)
        )

        let decoded = try roundtripContent(content, expectedIndex: 4)

        guard case let .aliasResponse(requestMessageId, .success(decodedAlias)) = decoded else {
            Issue.record("Expected successful aliasResponse, got \(decoded)")
            return
        }

        #expect(requestMessageId == "message-1")
        #expect(decodedAlias == alias)
    }

    @Test("Create proof request encodes at ContentV1 index 12")
    func createProofRequestMessageRoundtrip() throws {
        let request = PolkadotHostRemoteMessage.CreateProofRequest(
            callingProductId: "caller.product",
            context: ProductProofContext(productId: "target.product", suffix: .index(1)),
            ring: RingLocation(
                chainId: Data.random(of: 32)!,
                junctions: [.collectionId(Data.random(of: 32)!)]
            ),
            message: Data.random(of: 64)!
        )

        let content = PolkadotHostRemoteMessage.ContentV1.createProofRequest(request)
        let decoded = try roundtripContent(content, expectedIndex: 12)

        guard case let .createProofRequest(decodedRequest) = decoded else {
            Issue.record("Expected createProofRequest, got \(decoded)")
            return
        }

        #expect(decodedRequest.callingProductId == request.callingProductId)
        #expect(decodedRequest.context == request.context)
        #expect(decodedRequest.ring == request.ring)
        #expect(decodedRequest.message == request.message)
    }

    @Test("Create proof response encodes at ContentV1 index 13")
    func createProofResponseMessageRoundtrip() throws {
        let proof = RingVrfProof(
            proof: Data.random(of: 785)!,
            contextualAlias: ContextualAlias(
                context: Data.random(of: 32)!,
                alias: Data.random(of: 33)!
            ),
            ringIndex: 2,
            ringRevision: 9
        )

        let content = PolkadotHostRemoteMessage.ContentV1.createProofResponse(
            requestMessageId: "message-2",
            result: .success(proof)
        )

        let decoded = try roundtripContent(content, expectedIndex: 13)

        guard case let .createProofResponse(requestMessageId, .success(decodedProof)) = decoded else {
            Issue.record("Expected successful createProofResponse, got \(decoded)")
            return
        }

        #expect(requestMessageId == "message-2")
        #expect(decodedProof == proof)
    }

    @Test("Create proof failure encodes the RingVrfError variant on the wire")
    func createProofFailureRoundtrip() throws {
        let content = PolkadotHostRemoteMessage.ContentV1.createProofResponse(
            requestMessageId: "message-3",
            result: .failure(.notMember)
        )

        let decoded = try roundtripContent(content, expectedIndex: 13)

        guard case let .createProofResponse(_, .failure(error)) = decoded else {
            Issue.record("Expected failed createProofResponse, got \(decoded)")
            return
        }

        #expect(error == .notMember)
    }

    @Test("RingVrfError Unknown carries its reason across the wire")
    func ringVrfErrorUnknownRoundtrip() throws {
        let content = PolkadotHostRemoteMessage.ContentV1.createProofResponse(
            requestMessageId: "message-4",
            result: .failure(.unknown("boom"))
        )

        let decoded = try roundtripContent(content, expectedIndex: 13)

        guard case let .createProofResponse(_, .failure(error)) = decoded else {
            Issue.record("Expected failed createProofResponse, got \(decoded)")
            return
        }

        #expect(error == .unknown("boom"))
    }

    @Test("RingLocation decodes from the product wire JSON")
    func ringLocationJsonDecoding() throws {
        let chainId = Data.random(of: 32)!
        let collectionId = Data.random(of: 32)!
        let json = """
        {
            "chainId": "\(chainId.toHex(includePrefix: true))",
            "junctions": [
                { "tag": "PalletInstance", "value": 42 },
                { "tag": "CollectionId", "value": "\(collectionId.toHex(includePrefix: true))" }
            ]
        }
        """

        let decoded = try JSONDecoder().decode(RingLocation.self, from: Data(json.utf8))

        #expect(decoded.chainId == chainId)
        #expect(decoded.junctions == [.palletInstance(42), .collectionId(collectionId)])
        #expect(decoded.palletInstance == 42)
        #expect(decoded.collectionId == collectionId)
    }

    @Test("ProductProofContext decodes an index suffix from the product wire JSON")
    func productProofContextJsonDecoding() throws {
        let json = """
        { "productId": "test.product", "suffix": 7 }
        """

        let decoded = try JSONDecoder().decode(ProductProofContext.self, from: Data(json.utf8))

        #expect(decoded.productId == "test.product")
        #expect(decoded.suffix == .index(7))
    }

    @Test("ProductProofContext decodes a raw 32-byte suffix from hex")
    func productProofContextRawSuffixJsonDecoding() throws {
        let raw = Data.random(of: 32)!
        let json = """
        { "productId": "test.product", "suffix": "\(raw.toHex(includePrefix: true))" }
        """

        let decoded = try JSONDecoder().decode(ProductProofContext.self, from: Data(json.utf8))

        #expect(decoded.suffix == .raw(raw))
    }

    @Test("ProductProofContext rejects raw suffixes that are not 32 bytes")
    func productProofContextShortRawSuffixFails() throws {
        let json = """
        { "productId": "test.product", "suffix": "0x0102" }
        """

        #expect(throws: (any Error).self) {
            _ = try JSONDecoder().decode(ProductProofContext.self, from: Data(json.utf8))
        }
    }

    // MARK: - Helpers

    private func roundtripContent(
        _ content: PolkadotHostRemoteMessage.ContentV1,
        expectedIndex: UInt8
    ) throws -> PolkadotHostRemoteMessage.ContentV1 {
        let encoder = ScaleEncoder()
        try content.encode(scaleEncoder: encoder)
        let encoded = encoder.encode()

        #expect(encoded.first == expectedIndex)

        let decoder = try ScaleDecoder(data: encoded)
        return try PolkadotHostRemoteMessage.ContentV1(scaleDecoder: decoder)
    }
}
