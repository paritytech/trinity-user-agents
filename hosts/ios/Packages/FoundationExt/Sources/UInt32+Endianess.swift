import Foundation

public extension UInt32 {
    var bigEndianBytes: [UInt8] {
        var value = bigEndian

        return withUnsafeBytes(of: &value, Array.init)
    }

    var littleEndianBytes: [UInt8] {
        var value = littleEndian

        return withUnsafeBytes(of: &value, Array.init)
    }
}
