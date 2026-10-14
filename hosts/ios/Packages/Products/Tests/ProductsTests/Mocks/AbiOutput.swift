import Foundation

/// Hand-built Solidity ABI return data, so the DotNs tests pin the wire format without an encoder.
enum AbiOutput {
    static func word(_ value: Int) -> Data {
        Data(repeating: 0, count: 31) + Data([UInt8(value)])
    }

    static func dynamicBytes(_ bytes: Data) -> Data {
        let padding = (32 - bytes.count % 32) % 32
        return word(32) + word(bytes.count) + bytes + Data(repeating: 0, count: padding)
    }

    static func string(_ value: String) -> Data {
        dynamicBytes(Data(value.utf8))
    }

    static func address(_ bytes: Data) -> Data {
        Data(repeating: 0, count: 12) + bytes
    }
}
