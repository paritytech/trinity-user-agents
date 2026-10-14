import Foundation
import SubstrateSdk
import Operation_iOS

public struct AllowanceRecord: Equatable {
    public let accountId: AccountId
    public let allocatedAt: Date
    public let kind: Kind?
    public let priority: Priority
    public let latestRenewedPeriod: UInt32?

    public init(
        accountId: AccountId,
        allocatedAt: Date,
        kind: Kind?,
        priority: Priority,
        latestRenewedPeriod: UInt32?
    ) {
        self.accountId = accountId
        self.allocatedAt = allocatedAt
        self.kind = kind
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

    enum Priority: Int, Comparable {
        case normal = 0
        case high = 1

        public static func < (lhs: Self, rhs: Self) -> Bool {
            lhs.rawValue < rhs.rawValue
        }
    }
}
