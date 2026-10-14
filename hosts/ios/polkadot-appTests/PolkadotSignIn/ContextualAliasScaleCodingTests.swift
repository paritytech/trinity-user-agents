import Foundation
import Products
import SubstrateSdk
import Testing

@Suite("ContextualAlias SCALE Coding Tests")
struct ContextualAliasScaleCodingTests {
    @Test("context is fixed 32 raw bytes, alias is length-prefixed")
    func contextIsFixedBytesAliasIsVec() throws {
        // Spec wire shape (truapi v01): context is `[u8; 32]` (32 raw bytes, no prefix),
        // alias is `Vec<u8>` (compact length + bytes). JS decodes `S.Hex(32)` + `S.Hex()`.
        let context = Data.random(of: 32)!
        let original = ContextualAlias(context: context, alias: Data.random(of: 33)!)

        let encoder = ScaleEncoder()
        try original.encode(scaleEncoder: encoder)
        let encoded = encoder.encode()

        // No length prefix on context: the first 32 bytes are the context verbatim.
        #expect(encoded.prefix(32) == context)
        // compact(33) == 33 << 2 == 0x84 introduces the alias.
        #expect(encoded[32] == 0x84)
        #expect(encoded.count == 32 + 1 + 33)
    }

    @Test("round-trips through the fixed-bytes codec")
    func roundTrips() throws {
        let original = ContextualAlias(context: Data.random(of: 32)!, alias: Data.random(of: 40)!)

        let encoder = ScaleEncoder()
        try original.encode(scaleEncoder: encoder)
        let decoded = try ContextualAlias(scaleDecoder: ScaleDecoder(data: encoder.encode()))

        #expect(decoded == original)
    }
}
