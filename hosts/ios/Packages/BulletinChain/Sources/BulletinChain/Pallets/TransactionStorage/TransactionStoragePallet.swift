import Foundation
import SubstrateSdk

public enum TransactionStoragePallet {
    public static let name = "TransactionStorage"

    public enum AuthorizationScope: Encodable, Equatable {
        case account(AccountId)

        public func encode(to encoder: Encoder) throws {
            var container = encoder.unkeyedContainer()

            switch self {
            case let .account(accountId):
                try container.encode("Account")
                try container.encode(BytesCodable(wrappedValue: accountId))
            }
        }
    }

    public struct AuthorizationExtent: Decodable, Equatable {
        /// Transactions consumed so far.
        @StringCodable public var transactions: UInt32

        /// Total transaction allowance granted.
        @StringCodable public var transactionsAllowance: UInt32

        /// Bytes consumed by `store` calls (temporary storage).
        @StringCodable public var bytes: UInt64

        /// Total byte allowance granted.
        @StringCodable public var bytesAllowance: UInt64

        public var remainedTransactions: UInt32 {
            transactionsAllowance >= transactions ? transactionsAllowance - transactions : 0
        }

        public var remainedBytes: UInt64 {
            bytesAllowance >= bytes ? bytesAllowance - bytes : 0
        }
    }

    public struct Authorization: Decodable, Equatable {
        public let extent: AuthorizationExtent

        // block number starting from which submission is not allowed anymore
        @StringCodable public var expiration: BlockNumber
    }
}
