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

        ///  a user might become a full person while already has established chat
        ///  chat system doesn't support key change on the fly so always return lite one for now
        public var chatKey: Data? {
            lite?.chat?.wrappedValue
        }

        public func listedLabel(among labels: Set<Data>, assigned: Set<Data>) -> Data? {
            if let full = full?.label {
                let cachedLabels = [full, lite?.label].compactMap { $0 }
                return cachedLabels.contains(where: labels.contains) ? full : nil
            }
            guard let lite = lite?.label else {
                return nil
            }
            let assignedFull = assigned.filter(Self.isFullPersonLabel).min { $0.lexicographicallyPrecedes($1) }
            return assignedFull ?? (labels.contains(lite) ? lite : nil)
        }

        private static func isFullPersonLabel(_ label: Data) -> Bool {
            let letters = UInt8(ascii: "a") ... UInt8(ascii: "z")
            return !label.isEmpty && label.count <= 32 && label.allSatisfy(letters.contains)
        }
    }

    /// One account's name record together with the account it was read for.
    struct AccountNameWithAccountId {
        public let accountId: AccountId
        public let record: AccountNameRecord
    }
}
