import Foundation

// Must be in sync with Coinage pallet
enum RecyclerCollectionIdentifier {
    /// `RECYCLER_COLLECTION_PREFIX` from the Coinage pallet: `b"coinage/recycler"` (16 bytes)
    private static let prefix = Data("coinage/recycler".utf8)

    /// Builds the 32-byte collection identifier for a recycler of the given instance and coin value.
    ///
    /// Layout: `prefix[0..<16]` + `instanceId` little-endian u32 `[16..<20]` + `coinValue[20]`,
    /// zero-padded to 32. The instance id was inserted in individuality v0.12.0, which pushed
    /// the denomination from byte 16 to byte 20.
    static func identifier(instanceId: CoinageInstanceId, for coinValue: Int16) -> Data {
        var data = Data(repeating: 0, count: 32)
        data.replaceSubrange(0 ..< 16, with: prefix)
        data.replaceSubrange(16 ..< 20, with: instanceId.littleEndianBytes)
        data[20] = UInt8(clamping: coinValue)

        return data
    }
}
