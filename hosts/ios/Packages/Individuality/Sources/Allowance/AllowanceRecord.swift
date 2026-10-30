import Foundation
import SubstrateSdk
import Operation_iOS

public struct AllowanceRecord: Equatable {
    public let accountId: AccountId
    public let allocatedAt: Date
    public let kind: Kind?
    public let domain: Domain
    public let priority: Priority
    public let latestRenewedPeriod: UInt32?

    public init(
        accountId: AccountId,
        allocatedAt: Date,
        kind: Kind?,
        domain: Domain,
        priority: Priority,
        latestRenewedPeriod: UInt32?
    ) {
        self.accountId = accountId
        self.allocatedAt = allocatedAt
        self.kind = kind
        self.domain = domain
        self.priority = priority
        self.latestRenewedPeriod = latestRenewedPeriod
    }
}

extension AllowanceRecord: Identifiable {
    public var identifier: String { accountId.toHex() }
}

public extension AllowanceRecord {
    enum Kind {
        case pgas
        case bulletin
        case statementStore
    }

    /// The feature that owns the allowance, so cleanup can release one feature's records without touching another's.
    enum Domain: Equatable {
        case chat
        case device
        case product
        case coinage
    }

    enum Priority: Int, Comparable {
        case normal = 0
        case high = 1
        /// Never evicted, since losing the slot evicts statements a peer still needs.
        case critical = 2

        public var isEvictable: Bool {
            self != .critical
        }

        public static func < (lhs: Self, rhs: Self) -> Bool {
            lhs.rawValue < rhs.rawValue
        }
    }
}
