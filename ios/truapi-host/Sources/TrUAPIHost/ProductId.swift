import Foundation

/// A product identifier validated and normalized by the Rust core.
public struct ProductId: Hashable, Sendable, Codable, CustomStringConvertible {
    public let rawValue: String

    public init(_ value: String) throws {
        self = try parseProductId(input: value)
    }

    init(fromRust value: String) {
        rawValue = value
    }

    public var description: String { rawValue }

    public init(from decoder: Decoder) throws {
        try self.init(decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(rawValue)
    }
}
