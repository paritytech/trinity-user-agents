import Foundation
import SubstrateSdk

public extension PeopleLitePallet {
    enum AuthData: Codable {
        case asLightPerson(AccountNonce)

        public init(from decoder: any Decoder) throws {
            var container = try decoder.unkeyedContainer()

            let type = try container.decode(String.self)

            switch type {
            case "AsLitePerson":
                let nonce = try container.decode(StringCodable<AccountNonce>.self).wrappedValue
                self = .asLightPerson(nonce)
            default:
                throw DecodingError.dataCorruptedError(
                    in: container,
                    debugDescription: "unexpected auth type"
                )
            }
        }

        public func encode(to encoder: any Encoder) throws {
            var container = encoder.unkeyedContainer()

            switch self {
            case let .asLightPerson(nonce):
                try container.encode("AsLitePerson")
                try container.encode(StringCodable(wrappedValue: nonce))
            }
        }
    }

    struct TransactionExtension: Codable {
        let auth: AuthData

        public init(auth: AuthData) {
            self.auth = auth
        }

        public init(nonce: AccountNonce) {
            auth = .asLightPerson(nonce)
        }

        public init(from decoder: any Decoder) throws {
            auth = try AuthData(from: decoder)
        }

        public func encode(to encoder: any Encoder) throws {
            try auth.encode(to: encoder)
        }
    }
}

extension PeopleLitePallet.TransactionExtension: OnlyExplicitTransactionExtending {
    public var txExtensionId: String { "PeopleLiteAuth" }
}
