import Foundation

public extension Xcm {
    struct Version5<W>: Equatable, Codable where W: Equatable & XcmUniCodable {
        public let wrapped: W

        public init(wrapped: W) {
            self.wrapped = wrapped
        }

        public init(from decoder: any Decoder) throws {
            wrapped = try W(from: decoder, configuration: .V5)
        }

        public func encode(to encoder: any Encoder) throws {
            try wrapped.encode(to: encoder, configuration: .V5)
        }
    }
}
