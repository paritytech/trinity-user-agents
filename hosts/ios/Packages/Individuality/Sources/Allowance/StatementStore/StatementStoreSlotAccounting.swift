import Foundation
import SubstrateSdk

public protocol StatementStoreSlotAccounting {
    func deleteFor(accountId: AccountId) async throws
    func staleRows(currentPeriod: UInt32) async throws -> [AllowanceRecord]
    func markRenewed(_ record: AllowanceRecord, period: UInt32, since: Date) async throws
    func priorities(for accountIds: [AccountId]) async throws -> [AccountId: AllowanceRecord.Priority]
}
