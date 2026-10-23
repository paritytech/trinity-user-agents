import Foundation
import SubstrateSdk

public protocol StatementAccountFunding {
    /// Local-only: whether the account's record was allocated or renewed for `period` or later.
    /// It does not read the chain, so an on-chain eviction is not reflected.
    func hasAllocation(accountId: AccountId, coveringPeriod period: UInt32) async throws -> Bool

    /// Funds `accountId` for the current period unless it already has a slot.
    func ensureFunded(
        accountId: AccountId,
        priority: AllowanceRecord.Priority,
        domain: AllowanceRecord.Domain
    ) async throws
}
