import Foundation

public struct StatementSubmitParameters: Encodable {
    public let encodedStatement: Data

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.unkeyedContainer()
        try container.encode(encodedStatement.toHex(includePrefix: true))
    }
}
