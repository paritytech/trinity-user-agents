import Foundation
import SubstrateSdk

public extension ResourcesPallet {
    enum Credibility: Decodable, Equatable {
        case lite
        case person

        public init(from decoder: Decoder) throws {
            var container = try decoder.unkeyedContainer()
            let stringValue = try container.decode(String.self)

            switch stringValue {
            case "Lite":
                self = .lite
            case "Person":
                self = .person
            default:
                throw DecodingError.dataCorruptedError(
                    in: container,
                    debugDescription: "Unsupported string value: \(stringValue)"
                )
            }
        }
    }
}
