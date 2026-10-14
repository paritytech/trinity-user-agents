import Foundation
import SubstrateSdk

/// A single coin entry in a private payment transfer memo.
/// Contains the private key needed for recipient to claim.
public struct TransferCoinEntry: Equatable, ScaleCodable {
    /// Raw 32-byte private key for this coin
    public let privateKey: Data

    /// Denomination exponent (e.g., -2 for 0.01)
    public let exponent: Int16

    /// Fresh from Split/Unload
    public let age: Int32

    public init(privateKey: Data, exponent: Int16, age: Int32) {
        self.privateKey = privateKey
        self.exponent = exponent
        self.age = age
    }

    public init(scaleDecoder: any ScaleDecoding) throws {
        privateKey = try scaleDecoder.readAndConfirm(count: 32)
        exponent = try Int16(scaleDecoder: scaleDecoder)
        age = try Int32(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        scaleEncoder.appendRaw(data: privateKey)
        try exponent.encode(scaleEncoder: scaleEncoder)
        try age.encode(scaleEncoder: scaleEncoder)
    }
}
