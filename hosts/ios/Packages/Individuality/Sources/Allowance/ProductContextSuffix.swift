import Foundation
import FoundationExt
import SubstrateSdk

/// A product-scoped ring VRF suffix and the context derived from it.
///
/// Mirrors `ProductContextSuffix` and `build_product_context` in individuality
/// `support/src/context.rs`. A wrong input produces a valid-looking alias that only the chain
/// rejects — change nothing here without a matching change in the pallet.
enum ProductContextSuffix {
    /// `resources::notification` — family 1.
    case notificationSlot(period: UInt32, seq: UInt8)
    /// `personhood::statement_store_slot` — family 2.
    case statementStoreSlot(period: UInt32, seq: UInt32)
    /// `personhood::long_term_storage` — family 3.
    case longTermStorage(period: UInt32, counter: UInt8)
    /// `personhood::pgas_claim` — family 4.
    case pgasClaim(day: UInt32, slot: UInt32)

    /// `personhood::PRODUCT_NAME`, shared by every network.
    private static let productName = Data("peopl".utf8)
    private static let productPrefix = Data("product/".utf8)
    private static let systemPrefix = Data("sys/".utf8)
    private static let suffixLength = 32

    /// Family number for `resources::notification`.
    private static let notificationFamily: UInt32 = 1
    /// Family number for `personhood::statement_store_slot`.
    private static let statementStoreSlotFamily: UInt32 = 2
    /// Family number for `personhood::long_term_storage`.
    private static let longTermStorageFamily: UInt32 = 3
    /// Family number for `personhood::pgas_claim`.
    private static let pgasClaimFamily: UInt32 = 4

    /// The 32-byte `Raw` system suffix:
    /// `"sys/" ++ LE(family) ++ LE(first) ++ tail`, zero-padded to 32 bytes.
    var bytes: Data {
        switch self {
        case let .notificationSlot(period, seq):
            Self.rawSuffix(family: Self.notificationFamily, first: period, tail: seq)
        case let .statementStoreSlot(period, seq):
            Self.rawSuffix(family: Self.statementStoreSlotFamily, first: period, second: seq)
        case let .longTermStorage(period, counter):
            Self.rawSuffix(family: Self.longTermStorageFamily, first: period, tail: counter)
        case let .pgasClaim(day, slot):
            Self.rawSuffix(family: Self.pgasClaimFamily, first: day, second: slot)
        }
    }

    /// `blake2_256("product/" ++ productName ++ "." ++ networkSuffix ++ "/" ++ bytes)`
    func context(networkSuffix: Data) throws -> Data {
        var preimage = Self.productPrefix
        preimage.append(Self.productName)
        preimage.append(UInt8(ascii: "."))
        preimage.append(networkSuffix)
        preimage.append(UInt8(ascii: "/"))
        preimage.append(bytes)

        return try preimage.blake2b32()
    }
}

extension ProductContextSuffix {
    /// `"sys/" ++ LE(family) ++ LE(first) ++ LE(second)`, zero-padded to 32 bytes.
    private static func rawSuffix(family: UInt32, first: UInt32, second: UInt32) -> Data {
        var suffix = Data(repeating: 0, count: suffixLength)
        suffix.replaceSubrange(0 ..< 4, with: systemPrefix)
        suffix.replaceSubrange(4 ..< 8, with: family.littleEndianBytes)
        suffix.replaceSubrange(8 ..< 12, with: first.littleEndianBytes)
        suffix.replaceSubrange(12 ..< 16, with: second.littleEndianBytes)

        return suffix
    }

    /// `"sys/" ++ LE(family) ++ LE(first) ++ tail`, zero-padded to 32 bytes.
    ///
    /// The single-byte tail is indistinguishable from a little-endian `UInt32` of the same value,
    /// because the three bytes after it are zero either way. It is written as one byte to match
    /// the pallet exactly.
    private static func rawSuffix(family: UInt32, first: UInt32, tail: UInt8) -> Data {
        var suffix = Data(repeating: 0, count: suffixLength)
        suffix.replaceSubrange(0 ..< 4, with: systemPrefix)
        suffix.replaceSubrange(4 ..< 8, with: family.littleEndianBytes)
        suffix.replaceSubrange(8 ..< 12, with: first.littleEndianBytes)
        suffix[12] = tail

        return suffix
    }
}
