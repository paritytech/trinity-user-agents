import Foundation
import SubstrateSdk
import SubstrateSdkExt

public extension ResourcesPallet {
    struct RegisterLightPersonCall: Codable {
        enum CodingKeys: String, CodingKey {
            case identifierKey = "identifier_key"
            case username
            case reservedUsername = "reserved_username"
        }

        @BytesCodable public var identifierKey: Data
        @BytesCodable public var username: Data
        @NullCodable public var reservedUsername: Data?

        public func runtimeCall() -> RuntimeCall<Self> {
            RuntimeCall(
                moduleName: ResourcesPallet.name,
                callName: "register_lite_person",
                args: self
            )
        }
    }

    struct RemoveExpiredReservationCall: Codable {
        @BytesCodable public var username: Data
        @BytesCodable public var account: AccountId

        public init(username: Data, account: AccountId) {
            _username = BytesCodable(wrappedValue: username)
            _account = BytesCodable(wrappedValue: account)
        }

        public func runtimeCall() -> RuntimeCall<Self> {
            RuntimeCall(
                moduleName: ResourcesPallet.name,
                callName: "remove_expired_username_reservation",
                args: self
            )
        }
    }

    struct RegisterPersonCall: Codable {
        enum CodingKeys: String, CodingKey {
            case linkedLiteIdentity = "linked_lite_identity"
            case liteIdentityProof = "lite_identity_proof"
            case usernameChoice = "username_choice"
        }

        @BytesCodable public var linkedLiteIdentity: Data
        public let liteIdentityProof: MultiSignature
        public let usernameChoice: UsernameChoice

        public init(linkedLiteIdentity: Data, liteIdentityProof: MultiSignature, usernameChoice: UsernameChoice) {
            _linkedLiteIdentity = BytesCodable(wrappedValue: linkedLiteIdentity)
            self.liteIdentityProof = liteIdentityProof
            self.usernameChoice = usernameChoice
        }

        public func runtimeCall() -> RuntimeCall<Self> {
            RuntimeCall(
                moduleName: ResourcesPallet.name,
                callName: "register_person",
                args: self
            )
        }
    }

    /// call_index: 12
    /// Origin: LongTermStorageClaim(alias, collection) via AsResources(ClaimLongTermStorage(..))
    /// counter: 0..LongTermStorageClaimsPerPeriod-1; each counter produces distinct alias
    struct ClaimLongTermStorageCall: RuntimeCallConvertible {
        public var moduleName: String { ResourcesPallet.name }
        public var name: String { "claim_long_term_storage" }

        enum CodingKeys: String, CodingKey {
            case period
            case counter
            case accountId = "account_id"
        }

        @StringCodable public var period: UInt32
        @StringCodable public var counter: UInt8
        @BytesCodable public var accountId: Data

        public init(period: UInt32, counter: UInt8, accountId: Data) {
            self.period = period
            self.counter = counter
            _accountId = BytesCodable(wrappedValue: accountId)
        }
    }

    /// call_index: 10
    /// Origin: StmtStoreAlias via AsResources(RegisterStatementStoreAllowance(..))
    struct SetStatementStoreAccountCall: RuntimeCallConvertible {
        public var moduleName: String { ResourcesPallet.name }
        public var name: String { "set_statement_store_account" }

        enum CodingKeys: String, CodingKey {
            case period
            case seq
            case targetAccount = "target_account"
        }

        @StringCodable public var period: UInt32
        @StringCodable public var seq: UInt32
        @BytesCodable public var targetAccount: Data

        public init(period: UInt32, seq: UInt32, targetAccount: Data) {
            _period = StringCodable(wrappedValue: period)
            _seq = StringCodable(wrappedValue: seq)
            _targetAccount = BytesCodable(wrappedValue: targetAccount)
        }
    }

    /// Origin: NotificationAlias via AsResources(RegisterNotificationForCollection(..)).
    /// Binds `accountId` to one statement of the notification allowance until the period ends plus grace.
    struct SetNotificationStatementAccountForSequenceCall: RuntimeCallConvertible {
        public var moduleName: String { ResourcesPallet.name }
        public var name: String { "set_notification_statement_account_for_sequence" }

        enum CodingKeys: String, CodingKey {
            case reference
            case accountId = "account_id"
        }

        public let reference: NotificationReference
        @BytesCodable public var accountId: Data

        public init(reference: NotificationReference, accountId: Data) {
            self.reference = reference
            _accountId = BytesCodable(wrappedValue: accountId)
        }
    }

    struct NotificationReference: Codable, Equatable {
        @StringCodable public var period: UInt32
        @StringCodable public var seq: UInt8

        public init(period: UInt32, seq: UInt8) {
            _period = StringCodable(wrappedValue: period)
            _seq = StringCodable(wrappedValue: seq)
        }
    }

    enum UsernameChoice: Codable {
        case standalone(username: BytesCodable)
        case reservation(username: BytesCodable)

        public init(from decoder: Decoder) throws {
            var container = try decoder.unkeyedContainer()

            let state = try container.decode(String.self)

            switch state {
            case "Standalone":
                let username = try container.decode(BytesCodable.self)
                self = .standalone(username: username)
            case "Reservation":
                let username = try container.decode(BytesCodable.self)
                self = .reservation(username: username)
            default:
                throw DecodingError.dataCorruptedError(
                    in: container,
                    debugDescription: "Unsupported choice: \(state)"
                )
            }
        }

        public func encode(to encoder: any Encoder) throws {
            var container = encoder.unkeyedContainer()

            switch self {
            case let .standalone(username):
                try container.encode("Standalone")
                try container.encode(username)
            case let .reservation(username):
                try container.encode("Reservation")
                try container.encode(username)
            }
        }
    }
}
