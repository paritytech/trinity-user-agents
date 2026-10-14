import Foundation

public enum TransactionValidityError: Equatable, Decodable {
    case invalid(InvalidTransaction)
    case unknown(UnknownTransaction)

    public init(from decoder: Decoder) throws {
        var container = try decoder.unkeyedContainer()
        let name = try container.decode(String.self)

        switch name {
        case "Invalid":
            self = try .invalid(container.decode(InvalidTransaction.self))
        case "Unknown":
            self = try .unknown(container.decode(UnknownTransaction.self))
        default:
            throw DecodingError.dataCorrupted(
                .init(
                    codingPath: container.codingPath,
                    debugDescription: "Unsupported TransactionValidityError variant: \(name)"
                )
            )
        }
    }
}
