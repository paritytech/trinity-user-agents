import Foundation
import KeyDerivation
import Products
import SubstrateSdk
import Testing

@Suite("ProductProofContext Tests")
struct ProductProofContextTests {
    // The suffix expands to the 32-byte derivation index, so
    // context bytes are blake2b256(utf8("product/{id}/") ++ index_bytes(suffix)).
    // iOS is the reference implementation for this vector — Android pins against it.
    @Test("Context bytes match the cross-platform vector")
    func contextBytesMatchesRfcVector() throws {
        let context = ProductProofContext(productId: "voting.dot", suffix: .index(5))

        let expected = try Data(
            hexString: "0x28fbecf75aae5c703268e5d6de12a6df21f42e3ca29467c7c68d8e67e2657baa"
        )

        #expect(try context.contextBytes() == expected)
    }

    @Test("Default-account context matches Android's pinned vector")
    func contextBytesMatchesAndroidVector() throws {
        // Same pin as Android's ProductProofContextTest:
        // blake2b256(utf8("product/voting.dot/") ++ index_bytes(0)).
        let context = ProductProofContext(productId: "voting.dot", suffix: .index(0))

        let expected = try Data(
            hexString: "0xfc8e5a62a2abf020f4f5bc5d00c06c18404674804c8dacd5198357c5c761440d"
        )

        #expect(try context.contextBytes() == expected)
    }

    @Test("Index and raw forms of the same 32-byte index derive identical contexts")
    func indexAndRawFormsAreInterchangeable() throws {
        let index = DerivationIndex32(index: 5)
        let indexed = ProductProofContext(productId: "voting.dot", suffix: .index(5))
        let raw = ProductProofContext(productId: "voting.dot", suffix: .raw(index.bytes))

        #expect(try indexed.contextBytes() == raw.contextBytes())
    }

    @Test("Different products with the same suffix derive unlinkable contexts")
    func productScopingPreventsCollisions() throws {
        let first = ProductProofContext(productId: "a.dot", suffix: .index(0))
        let second = ProductProofContext(productId: "b.dot", suffix: .index(0))

        #expect(try first.contextBytes() != second.contextBytes())
    }

    @Test("Different suffixes derive different contexts for the same product")
    func suffixesAreDistinguished() throws {
        let first = ProductProofContext(productId: "a.dot", suffix: .index(0))
        let second = ProductProofContext(productId: "a.dot", suffix: .index(1))

        #expect(try first.contextBytes() != second.contextBytes())
    }
}
