import Foundation
import SubstrateSdk

public extension DotnsGatewayPallet {
    /// A single registered name held against an account.
    struct NameEntry: Decodable, Equatable {
        @BytesCodable public var label: Data
    }

    /// The value stored in the `AccountNames` map: the lite and full names of one account.
    ///
    /// The pallet writes this map at registration time and it is not the source of truth;
    /// the dotNS contracts are. It is read here because it is the only subscribable source and
    /// because it reflects a name that is registered but has not yet settled into the owner's
    /// `LabelStore`.
    struct AccountNameRecord: Decodable, Equatable {
        public let lite: NameEntry?
        public let full: NameEntry?

        /// The label to show for the account: the full name where one exists, otherwise the lite
        /// name, and `nil` while neither is registered.
        public var username: Data? {
            full?.label ?? lite?.label
        }
    }
}
