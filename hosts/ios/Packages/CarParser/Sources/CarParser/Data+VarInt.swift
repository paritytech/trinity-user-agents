import Foundation
import VarInt

enum VarIntBridgeError: Error {
    case unexpectedEndOfData
    case overflow
}

extension Data {
    /// Read unsigned LEB128 varint at the given byte offset.
    func readUVarInt(at offset: Int) throws -> (value: UInt64, bytesRead: Int) {
        let start = startIndex + offset
        let end = Swift.min(start + 10, endIndex) // LEB128 UInt64 is at most 10 bytes
        let slice = Array(self[start ..< end])
        let (value, bytesRead) = uVarInt(slice)
        if bytesRead == 0 { throw VarIntBridgeError.unexpectedEndOfData }
        if bytesRead < 0 { throw VarIntBridgeError.overflow }
        return (value, bytesRead)
    }

    /// Encode a UInt64 as unsigned LEB128 varint bytes.
    static func varInt(_ value: UInt64) -> Data {
        Data(putUVarInt(value))
    }
}
