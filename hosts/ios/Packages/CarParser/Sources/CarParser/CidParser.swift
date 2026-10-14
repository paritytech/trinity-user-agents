import CID
import Foundation

struct ParsedCid {
    let cid: CID
    let totalBytesRead: Int
}

enum CidParser {
    /// Parse a CID from binary data at the given offset.
    ///
    /// CIDv0: starts with `0x12 0x20` (sha2-256 multihash with 32-byte digest)
    /// CIDv1: `<version-varint> <codec-varint> <multihash>`
    static func parseCid(from data: Data, at offset: Int) throws -> ParsedCid {
        let byteCount = try cidByteCount(from: data, at: offset)
        let base = data.startIndex + offset
        let cidBytes = Data(data[base ..< (base + byteCount)])
        let cid = try CID(cidBytes)
        return ParsedCid(cid: cid, totalBytesRead: byteCount)
    }

    // MARK: - Private

    /// Compute how many bytes the CID occupies starting at `offset`.
    private static func cidByteCount(from data: Data, at offset: Int) throws -> Int {
        guard offset < data.count - 1 else {
            throw CIDError.cidStringTooShort
        }

        let base = data.startIndex + offset

        // CIDv0: sha2-256 multihash prefix (0x12 = sha2-256, 0x20 = 32 bytes)
        if data[base] == 0x12, data[base + 1] == 0x20 {
            let totalLen = 2 + 32
            guard offset + totalLen <= data.count else {
                throw CIDError.cidStringTooShort
            }
            return totalLen
        }

        // CIDv1: version varint + codec varint + multihash
        var pos = offset

        let (_, versionLen) = try data.readUVarInt(at: pos)
        pos += versionLen

        let (_, codecLen) = try data.readUVarInt(at: pos)
        pos += codecLen

        // Multihash: <hash-function-varint> <digest-size-varint> <digest-bytes>
        let (_, hashFnLen) = try data.readUVarInt(at: pos)
        pos += hashFnLen

        let (digestSize, digestSizeLen) = try data.readUVarInt(at: pos)
        pos += digestSizeLen

        guard pos + Int(digestSize) <= data.count else {
            throw CIDError.cidStringTooShort
        }
        pos += Int(digestSize)

        return pos - offset
    }
}
