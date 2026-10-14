import Foundation
import KeyDerivation
import Products
import SubstrateSdk
import Testing

/// Byte-level pins for the `Either<u32, [u8; 32]>` account selector.
/// iOS defines these encodings — they are the cross-platform contract.
@Suite("ProductAccountSelector Coding Tests")
struct ProductAccountSelectorCodingTests {
    @Test("Index form encodes as 0x00 followed by u32 little-endian")
    func indexFormScaleLayout() throws {
        let encoded = try encoded(ProductAccountSelector.index(5))

        #expect(encoded == Data([0x00, 0x05, 0x00, 0x00, 0x00]))
    }

    @Test("Raw form encodes as 0x01 followed by 32 raw bytes, no length prefix")
    func rawFormScaleLayout() throws {
        // Same fixed vector as Android's ProductAccountIdScaleTest (32 × 0xAB).
        let raw = Data(repeating: 0xAB, count: 32)

        let encoded = try encoded(ProductAccountSelector.raw(raw))

        #expect(encoded == Data([0x01]) + raw)
        #expect(encoded.toHex() == "01" + String(repeating: "ab", count: 32))
    }

    @Test("ProductAccountId tuple matches the cross-platform SCALE vector")
    func productAccountIdScaleVector() throws {
        // Pinned on Android (ProductAccountIdScaleTest) and generated from the JS SDK:
        // ProductAccountId.enc(['browse.dot', derivationIndexOf(5)]).
        let account = ProductAccountId(productId: "browse.dot", derivationIndex: .index(5))

        let encoder = ScaleEncoder()
        try account.encode(scaleEncoder: encoder)

        #expect(try encoder.encode() == Data(hexString: "0x2862726f7773652e646f740005000000"))
    }

    @Test("SCALE round-trips both forms")
    func scaleRoundTrip() throws {
        let selectors: [ProductAccountSelector] = [
            .index(0),
            .index(UInt32.max),
            .raw(Data.random(of: 32)!)
        ]

        for selector in selectors {
            let decoded = try ProductAccountSelector(
                scaleDecoder: ScaleDecoder(data: encoded(selector))
            )
            #expect(decoded == selector)
        }
    }

    @Test("Unknown SCALE variant byte fails to decode")
    func unknownVariantFailsDecoding() throws {
        #expect(throws: (any Error).self) {
            _ = try ProductAccountSelector(scaleDecoder: ScaleDecoder(data: Data([0x02, 0x00])))
        }
    }

    @Test("Raw form with wrong length fails to encode")
    func wrongLengthRawFailsEncoding() throws {
        #expect(throws: ProductAccountSelectorError.invalidRawLength(31)) {
            _ = try encoded(ProductAccountSelector.raw(Data.randomOrError(of: 31)))
        }
    }

    @Test("JSON number decodes as index, 0x-hex-32 string decodes as raw")
    func jsonDecoding() throws {
        // Same fixed vector as Android's DerivationIndexWireAdapterTest (32 × 0xAB).
        let raw = Data(repeating: 0xAB, count: 32)

        let index = try JSONDecoder().decode(
            [ProductAccountSelector].self,
            from: Data("[7]".utf8)
        )
        let rawDecoded = try JSONDecoder().decode(
            [ProductAccountSelector].self,
            from: Data("[\"0x\(String(repeating: "ab", count: 32))\"]".utf8)
        )

        #expect(index == [.index(7)])
        #expect(rawDecoded == [.raw(raw)])
    }

    @Test("JSON hex string that is not 32 bytes fails to decode")
    func shortJsonHexFailsDecoding() throws {
        #expect(throws: (any Error).self) {
            _ = try JSONDecoder().decode([ProductAccountSelector].self, from: Data("[\"0x0102\"]".utf8))
        }
    }

    @Test("JSON encoding mirrors the decoding forms")
    func jsonEncoding() throws {
        let raw = Data(repeating: 0xAB, count: 32)

        let indexJson = try JSONEncoder().encode([ProductAccountSelector.index(7)])
        let rawJson = try JSONEncoder().encode([ProductAccountSelector.raw(raw)])

        #expect(String(decoding: indexJson, as: UTF8.self) == "[7]")
        #expect(String(decoding: rawJson, as: UTF8.self) == "[\"0x\(String(repeating: "ab", count: 32))\"]")
    }

    @Test("index32 expands the u32 form and passes the raw form through")
    func index32Expansion() throws {
        let expected = try Data(
            hexString: "0x0500000012e86013736c5498f050b03cdc16957dff0e422fb92ca77ec3ab168f"
        )
        let raw = Data.random(of: 32)!

        #expect(try ProductAccountSelector.index(5).index32().bytes == expected)
        #expect(try ProductAccountSelector.raw(raw).index32().bytes == raw)
    }

    private func encoded(_ selector: ProductAccountSelector) throws -> Data {
        let encoder = ScaleEncoder()
        try selector.encode(scaleEncoder: encoder)
        return encoder.encode()
    }
}
