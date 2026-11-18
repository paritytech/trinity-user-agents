import Foundation
import SubstrateSdk
import BigInt

/// Transfer memo containing raw 64-byte private keys.
/// Recipient uses these keys to derive account keys and claim coins.
public struct TransferMemo: Equatable, ScaleCodable {
    public let entries: [Data]
    public let totalValue: Balance

    public init(entries: [Data], totalValue: Balance) {
        self.entries = entries
        self.totalValue = totalValue
    }

    public init(scaleDecoder: any ScaleDecoding) throws {
        entries = try [Data](scaleDecoder: scaleDecoder)
        totalValue = try BigUInt(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try entries.encode(scaleEncoder: scaleEncoder)
        try totalValue.encode(scaleEncoder: scaleEncoder)
    }
}
