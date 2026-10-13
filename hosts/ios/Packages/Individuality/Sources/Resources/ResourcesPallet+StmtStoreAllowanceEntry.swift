import Foundation
import SubstrateSdk

public extension ResourcesPallet {
    struct StatementAllowance: Decodable {
        @StringCodable public var maxCount: UInt32
        @StringCodable public var maxSize: UInt32
    }

    struct StmtStoreAllowanceEntry: Decodable {
        @BytesCodable public var accountId: Data
        @StringCodable public var seq: UInt32
        @StringCodable public var since: UInt64

        public init(accountId: Data, seq: UInt32, since: UInt64) {
            _accountId = BytesCodable(wrappedValue: accountId)
            self.seq = seq
            self.since = since
        }
    }
}
