import BigInt
import Foundation
import SubstrateSdk

public enum AssetsPallet {
    public static let name = "Assets"

    public enum AccountStatus: String, Decodable {
        case liquid = "Liquid"
        case frozen = "Frozen"
        case blocked = "Blocked"

        public init(from decoder: Decoder) throws {
            var container = try decoder.unkeyedContainer()

            let type = try container.decode(String.self)

            guard let value = AccountStatus(rawValue: type) else {
                throw DecodingError.dataCorruptedError(
                    in: container,
                    debugDescription: "Unexpected account status"
                )
            }

            self = value
        }
    }

    public struct Account: Decodable {
        @StringCodable public var balance: BigUInt
        public let status: AccountStatus

        public var isFrozen: Bool { !canSend }
        public var isBlocked: Bool { !canSend && !canReceive }

        public var canSend: Bool {
            switch status {
            case .liquid:
                true
            case .frozen,
                 .blocked:
                false
            }
        }

        public var canReceive: Bool {
            switch status {
            case .liquid,
                 .frozen:
                true
            case .blocked:
                false
            }
        }
    }

    public struct Details: Decodable {
        public enum Status: String, Decodable {
            case live = "Live"
            case frozen = "Frozen"
            case destroying = "Destroying"

            public init(from decoder: Decoder) throws {
                var container = try decoder.unkeyedContainer()

                let type = try container.decode(String.self)

                guard let value = Status(rawValue: type) else {
                    throw DecodingError.dataCorruptedError(
                        in: container,
                        debugDescription: "Unexpected asset status"
                    )
                }

                self = value
            }
        }

        public var isFrozen: Bool {
            status != .live
        }

        @StringCodable public var minBalance: BigUInt
        public let status: Status
        public let isSufficient: Bool
        @BytesCodable public var issuer: AccountId
    }
}
