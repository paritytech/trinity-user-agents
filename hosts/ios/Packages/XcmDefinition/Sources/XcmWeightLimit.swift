import Foundation
import SubstrateSdk

public extension Xcm {
    enum WeightLimitFields {
        public static let unlimited = "Unlimited"
        public static let limited = "Limited"
    }

    enum WeightLimit<T>: Codable where T: Codable {
        case unlimited
        case limited(weight: T)

        public func encode(to encoder: Encoder) throws {
            var container = encoder.unkeyedContainer()

            switch self {
            case .unlimited:
                try container.encode(WeightLimitFields.unlimited)
                try container.encode(JSON.null)
            case let .limited(weight):
                try container.encode(WeightLimitFields.limited)
                try container.encode(weight)
            }
        }

        public init(from decoder: Decoder) throws {
            var container = try decoder.unkeyedContainer()

            let type = try container.decode(String.self)

            switch type {
            case WeightLimitFields.unlimited:
                self = .unlimited
            case WeightLimitFields.limited:
                let weight = try container.decode(T.self)
                self = .limited(weight: weight)
            default:
                throw DecodingError.dataCorruptedError(
                    in: container,
                    debugDescription: "Unexpected type"
                )
            }
        }
    }
}

extension Xcm.WeightLimit: Equatable where T: Equatable {}

public extension Xcm.WeightLimit {
    func map<U: Codable>(_ transform: (T) throws -> U) rethrows -> Xcm.WeightLimit<U> {
        switch self {
        case let .limited(weight):
            try .limited(weight: transform(weight))
        case .unlimited:
            .unlimited
        }
    }
}
