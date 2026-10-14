import Foundation
import SubstrateSdk

public extension Xcm {
    struct FeesPaidEvent<M: Decodable>: Decodable {
        public let paying: JSON
        public let assets: [M]

        public init(from decoder: any Decoder) throws {
            var container = try decoder.unkeyedContainer()

            paying = try container.decode(JSON.self)
            assets = try container.decode([M].self)
        }
    }

    struct SentEvent<M: Decodable>: Decodable {
        public let origin: JSON
        public let destination: JSON
        public let message: M
        public let messageId: JSON

        public init(from decoder: any Decoder) throws {
            var container = try decoder.unkeyedContainer()

            origin = try container.decode(JSON.self)
            destination = try container.decode(JSON.self)
            message = try container.decode(M.self)
            messageId = try container.decode(JSON.self)
        }
    }
}
