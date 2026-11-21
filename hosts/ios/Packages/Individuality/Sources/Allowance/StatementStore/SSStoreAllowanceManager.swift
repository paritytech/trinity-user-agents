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
    public typealias Holder = StatementStoreAllowanceHolder

    public func allocate(
        holder: StatementStoreAllowanceHolder,
        policy: OnExistingAllowancePolicy,
        priority: AllowanceRecord.Priority,
        domain: AllowanceRecord.Domain
    ) async throws {
        try await issueDiagnostics.observeAllocation {
            try await allocateInBackground(
                AllocationRequest(holder: holder, policy: policy, priority: priority, domain: domain)
            )
        }
    }

    public func release(with strategy: AllowanceReleaseStrategy) async throws {
        try await backgroundExecutor.execute { [repository] in
            let identifiers: [String] =
                switch strategy {
                case let .accountIds(accountIds):
                    accountIds.map { $0.toHex() }
                case let .filter(filter):
                    try await filter.recordsToRelease(
                        from: repository.fetchAllOperation(with: RepositoryFetchOptions()).asyncExecute()
                    ).map(\.identifier)
                }

            guard !identifiers.isEmpty else { return }

            try await repository.saveOperation({ [] }, { identifiers }).asyncExecute()
        }
    }

    public func renew() async throws {
        try await backgroundExecutor.execute { [renewer] in try await renewer.renew() }
    }
}

extension SSStoreAllowanceManager: StatementAccountFunding {
    public func hasAllocation(accountId: AccountId, coveringPeriod period: UInt32) async throws -> Bool {
        let record = try await repository.fetchOperation(by: { accountId.toHex() }, options: .init()).asyncExecute()
        return record?.latestRenewedPeriod.map { $0 >= period } ?? false
    }

    public func ensureFunded(
        holder: StatementStoreAllowanceHolder,
        priority: AllowanceRecord.Priority,
        domain: AllowanceRecord.Domain
    ) async throws {
        try await issueDiagnostics.observeAllocation {
            try await allocateInBackground(
                AllocationRequest(holder: holder, policy: .ignore, priority: priority, domain: domain)
            )
        }
    }
}

private extension SSStoreAllowanceManager {
    struct AllocationRequest {
        let holder: StatementStoreAllowanceHolder
        let policy: OnExistingAllowancePolicy
        let priority: AllowanceRecord.Priority
        let domain: AllowanceRecord.Domain

        var accountId: AccountId { holder.accountId }
    }

    func allocateInBackground(_ request: AllocationRequest) async throws {
        try await backgroundExecutor.execute { [allocator, repository, slotInfoProvider] in
            try await markStallActivity("Allocating SS allowance") {
                let existing = try await markStallRegion("Check existing slot") {
                    try await slotInfoProvider.hasExistingSlot(for: request.accountId)
                }

                if request.policy == .ignore, existing {
                    return
                }

                let period = try await allocator.assignSlot(accountId: request.accountId, priority: request.priority)

                let record = AllowanceRecord(
                    accountId: request.accountId,
                    allocatedAt: Date(),
                    kind: .statementStore,
                    domain: request.domain,
                    priority: request.priority,
                    latestRenewedPeriod: period,
                    signerKeyId: request.holder.signerKeyId
                )
                try await repository.saveOperation({ [record] }, { [] }).asyncExecute()
            }
        }
    }
}
