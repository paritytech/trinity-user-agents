import Foundation
import SubstrateSdk
import Operation_iOS
import StructuredConcurrency
import BackgroundExecution

public final class PGASAllowanceManager {
    private let repository: AnyDataProviderRepository<AllowanceRecord>
    private let allocator: AllowanceSlotAllocating
    private let slotInfoProvider: PGASSlotInfoProviding
    private let backgroundExecutor: any BackgroundExecuting

    public init(
        repository: AnyDataProviderRepository<AllowanceRecord>,
        allocator: AllowanceSlotAllocating,
        slotInfoProvider: PGASSlotInfoProviding,
        backgroundExecutor: any BackgroundExecuting
    ) {
        self.repository = repository
        self.allocator = allocator
        self.slotInfoProvider = slotInfoProvider
        self.backgroundExecutor = backgroundExecutor
    }
}

extension PGASAllowanceManager: AllowanceManaging {
    public func allocate(
        accountId: AccountId,
        policy _: OnExistingAllowancePolicy,
        priority: AllowanceRecord.Priority
    ) async throws {
        // TODO: Should save allocation locally and check if we have it
        // always allocate new slot for now
//        if policy == .ignore, try await slotInfoProvider.hasExistingSlot(for: accountId) {
//            return
//        }

        try await backgroundExecutor.execute { [allocator, repository] in
            let period = try await allocator.assignSlot(accountId: accountId, priority: priority)

            let record = AllowanceRecord(
                accountId: accountId,
                allocatedAt: Date(),
                kind: .pgas,
                priority: priority,
                latestRenewedPeriod: period
            )
            try await repository.saveOperation({ [record] }, { [] }).asyncExecute()
        }
    }
}
