import Foundation
import SubstrateSdk
import Operation_iOS
import StructuredConcurrency
import BackgroundExecution

public final class SSStoreAllowanceManager {
    private let repository: AnyDataProviderRepository<AllowanceRecord>
    private let allocator: AllowanceSlotAllocating
    private let slotInfoProvider: StatementStoreSlotInfoProviding
    private let renewer: StatementStoreSlotRenewing
    private let backgroundExecutor: any BackgroundExecuting
    private let issueDiagnostics: AllowanceIssueDiagnostics

    public init(
        repository: AnyDataProviderRepository<AllowanceRecord>,
        allocator: AllowanceSlotAllocating,
        slotInfoProvider: StatementStoreSlotInfoProviding,
        renewer: StatementStoreSlotRenewing,
        backgroundExecutor: any BackgroundExecuting,
        issueDiagnostics: AllowanceIssueDiagnostics
    ) {
        self.repository = repository
        self.allocator = allocator
        self.slotInfoProvider = slotInfoProvider
        self.renewer = renewer
        self.backgroundExecutor = backgroundExecutor
        self.issueDiagnostics = issueDiagnostics
    }
}

extension SSStoreAllowanceManager: AllowanceManaging {
    public func allocate(
        accountId: AccountId,
        policy: OnExistingAllowancePolicy,
        priority: AllowanceRecord.Priority
    ) async throws {
        try await issueDiagnostics.observeAllocation {
            try await allocateInBackground(accountId: accountId, policy: policy, priority: priority)
        }
    }

    public func release(accountId: AccountId) async throws {
        try await backgroundExecutor.execute { [repository] in
            let identifier = accountId.toHex()
            try await repository.saveOperation({ [] }, { [identifier] }).asyncExecute()
        }
    }

    public func renew() async throws {
        try await backgroundExecutor.execute { [renewer] in try await renewer.renew() }
    }
}

private extension SSStoreAllowanceManager {
    func allocateInBackground(
        accountId: AccountId,
        policy: OnExistingAllowancePolicy,
        priority: AllowanceRecord.Priority
    ) async throws {
        try await backgroundExecutor.execute { [allocator, repository, slotInfoProvider] in
            try await markStallActivity("Allocating SS allowance") {
                let existing = try await markStallRegion("Check existing slot") {
                    try await slotInfoProvider.hasExistingSlot(for: accountId)
                }

                if policy == .ignore, existing {
                    return
                }

                let period = try await allocator.assignSlot(accountId: accountId, priority: priority)

                let record = AllowanceRecord(
                    accountId: accountId,
                    allocatedAt: Date(),
                    kind: .statementStore,
                    priority: priority,
                    latestRenewedPeriod: period
                )
                try await repository.saveOperation({ [record] }, { [] }).asyncExecute()
            }
        }
    }
}
