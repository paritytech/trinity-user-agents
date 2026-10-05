import Foundation
import SubstrateSdk

public extension DotnsGatewayPallet {
    /// A single registered name held against an account.
    struct NameEntry: Decodable, Equatable {
        @BytesCodable public var label: Data
        /// The ECDH chat key the dotNS contracts hold for this label, absent where the pallet
        /// never saw one: a label backfilled by migration, or a full label linked to a lite
        /// label other than the recorded one.
        public let chat: BytesCodable?
    }

    /// The value stored in the `AccountNames` map: a cache of lite and full names.
    ///
    /// The gateway pallet writes this map on `reserve_name` and `register_name`. The dotNS
    /// contracts are the source of truth. Names registered outside those paths do not appear,
    /// including pre-launch whitelist allocations. Contract-side label changes leave the map
    /// stale. It is read here because it is the only subscribable source across many accounts
    /// and reflects registered but not yet settled names. This map is temporary, going away
    /// when apps observe contract storage: https://github.com/paritytech/individuality-community/issues/52
    struct AccountNameRecord: Decodable, Equatable {
        public let lite: NameEntry?
        public let full: NameEntry?

        /// The label to show for the account: the full name where one exists, otherwise the lite
        /// name, and `nil` while neither is registered.
        public var username: Data? {
            full?.label ?? lite?.label
        }

        /// The chat key to reach the account with: whichever entry carries one, since a full
        /// label can be registered without the key its lite label already holds.
        public var chatKey: Data? {
            full?.chat?.wrappedValue ?? lite?.chat?.wrappedValue
        }
    }

    /// One account's name record together with the account it was read for.
    struct AccountNameWithAccountId {
        public let accountId: AccountId
        public let record: AccountNameRecord
    }
}
