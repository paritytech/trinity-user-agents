import Foundation
import FoundationExt
import SubstrateSdk

public enum DerivationIndex32Error: Error, Equatable {
    case invalidLength(expected: Int, actual: Int)
}

/// 32-byte derivation index identifying an account within a product subtree.
/// Plain `u32` indices expand to `u32_le_bytes(index) ++ INDEX_MAGIC`; raw 32-byte values
/// are the escape hatch for byte-valued selectors.
public struct DerivationIndex32: Hashable, Sendable {
    public static let length = 32

    /// `blake2b256("product-account-index")[..28]` — pinned here so index expansion is
    /// non-throwing; `DerivationIndex32Tests` asserts it against the live computation.
    static let indexMagic = Data([
        0x12, 0xE8, 0x60, 0x13, 0x73, 0x6C, 0x54, 0x98,
        0xF0, 0x50, 0xB0, 0x3C, 0xDC, 0x16, 0x95, 0x7D,
        0xFF, 0x0E, 0x42, 0x2F, 0xB9, 0x2C, 0xA7, 0x7E,
        0xC3, 0xAB, 0x16, 0x8F
    ])

    public let bytes: Data

    /// `index_bytes(index)` — little-endian `u32` followed by the index magic.
    public init(index: UInt32) {
        bytes = Data(index.littleEndianBytes) + Self.indexMagic
    }

    /// A raw 32-byte index; must be exactly 32 bytes.
    public init(raw: Data) throws {
        guard raw.count == Self.length else {
            throw DerivationIndex32Error.invalidLength(expected: Self.length, actual: raw.count)
        }

        bytes = raw
    }

    /// Renders the index as a derivation path segment. Hex is what keeps the 32 bytes
    /// intact: the junction parser maps a hex segment straight to the raw chain code,
    /// skipping the string normalization other forms go through.
    public func asPathSegment() -> String {
        bytes.toHex(includePrefix: true)
    }
}
