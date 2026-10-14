import Foundation
import SubstrateSdk
import Operation_iOS

public final class StatementStoreSlotAccountant: StatementStoreSlotAccounting {
    private let repository: AnyDataProviderRepository<AllowanceRecord>

    public init(repository: AnyDataProviderRepository<AllowanceRecord>) {
        self.repository = repository
    }

    public func deleteFor(accountId: AccountId) async throws {
        let identifier = accountId.toHex()
        try await repository.saveOperation({ [] }, { [identifier] }).asyncExecute()
    }

    public func staleRows(currentPeriod: UInt32) async throws -> [AllowanceRecord] {
        let allRecords = try await repository.fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
        return allRecords.filter { record in
            guard let period = record.latestRenewedPeriod else { return false }
            return period < currentPeriod
        }
    }

    public func markRenewed(_ record: AllowanceRecord, period: UInt32, since: Date) async throws {
        let updated = AllowanceRecord(
            accountId: record.accountId,
            allocatedAt: since,
            kind: record.kind,
            priority: record.priority,
            latestRenewedPeriod: period
        )
        try await repository.saveOperation({ [updated] }, { [] }).asyncExecute()
    }

    public func priorities(for accountIds: [AccountId]) async throws -> [AccountId: AllowanceRecord.Priority] {
        let allRecords = try await repository.fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
        let requestedIds = Set(accountIds)
        let filtered = allRecords.filter { requestedIds.contains($0.accountId) }

        return filtered.reduce(into: [:]) { $0[$1.accountId] = $1.priority }
    }
}
