import Foundation
import SubstrateSdk

public extension ResourcesPallet {
    struct ConsumerInfo: Decodable, Equatable {
        @BytesCodable public var identifierKey: Data
        @BytesCodable public var liteUsername: Data
        public let fullUsername: BytesCodable?
        public let credibility: Credibility

        public var username: Data {
            fullUsername?.wrappedValue ?? liteUsername
        }
    }

    struct ConsumerWithAccountId {
        public let accountId: AccountId
        public let info: ConsumerInfo
    }
}
