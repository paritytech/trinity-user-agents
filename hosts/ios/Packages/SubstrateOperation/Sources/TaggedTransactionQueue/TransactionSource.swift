import Foundation
import SubstrateSdk

public enum TransactionSource: String {
    case inBlock = "InBlock"
    case local = "Local"
    case external = "External"
}

extension TransactionSource: Encodable {
    public func encode(to encoder: Encoder) throws {
        var container = encoder.unkeyedContainer()
        try container.encode(rawValue)
        try container.encode(JSON.null)
    }
}
