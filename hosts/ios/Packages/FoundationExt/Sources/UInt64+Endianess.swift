import Foundation

public extension UInt64 {
    var littleEndianBytes: [UInt8] {
        var value = littleEndian

        return withUnsafeBytes(of: &value, Array.init)
    }
}
