import Foundation
import KeyDerivation
import SubstrateSdk
import Testing

/// Byte-level pins for `index_bytes(n)` — the cross-platform contract.
@Suite("DerivationIndex32 Tests")
struct DerivationIndex32Tests {
    @Test("index_bytes lays out u32-LE followed by the 28-byte magic")
    func indexBytesLayout() throws {
        let expectedMagic = try Data(
            hexString: "0x12e86013736c5498f050b03cdc16957dff0e422fb92ca77ec3ab168f"
        )

        let zero = DerivationIndex32(index: 0)
        let five = DerivationIndex32(index: 5)

        #expect(zero.bytes == Data([0x00, 0x00, 0x00, 0x00]) + expectedMagic)
        #expect(five.bytes == Data([0x05, 0x00, 0x00, 0x00]) + expectedMagic)
        #expect(zero.bytes.count == DerivationIndex32.length)
    }

    @Test("Magic is blake2b256(\"product-account-index\") truncated to 28 bytes")
    func magicDerivation() throws {
        let derived = try Data("product-account-index".utf8).blake2b32().prefix(28)

        let index = DerivationIndex32(index: 0)

        #expect(index.bytes.suffix(28) == derived)
    }

    @Test("Raw init requires exactly 32 bytes")
    func rawInitValidatesLength() throws {
        let valid = try Data.randomOrError(of: 32)

        #expect(try DerivationIndex32(raw: valid).bytes == valid)
        #expect(throws: DerivationIndex32Error.invalidLength(expected: 32, actual: 31)) {
            _ = try DerivationIndex32(raw: Data.randomOrError(of: 31))
        }
    }
}
