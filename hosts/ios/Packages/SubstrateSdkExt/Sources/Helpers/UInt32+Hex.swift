import Foundation
import SubstrateSdk

public extension UInt32 {
    /// Parses a hex string (with or without a `0x` prefix) into a `UInt32`, or `nil` when the value
    /// is not valid hex or overflows the type.
    static func fromHex(_ hex: String) -> UInt32? {
        UInt32(hex.withoutHexPrefix(), radix: 16)
    }
}
