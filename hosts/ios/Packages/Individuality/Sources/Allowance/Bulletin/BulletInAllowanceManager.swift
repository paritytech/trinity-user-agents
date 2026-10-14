import Foundation
import SubstrateSdk
import Operation_iOS
import StructuredConcurrency
import BackgroundExecution
import SDKLogger

public final class BulletInAllowanceManager {
    private let infoProvider: BulletInSlotInfoProviding
    private let allocator: AllowanceSlotAllocating
    private let backgroundExecutor: any BackgroundExecuting
    private let issueDiagnostics: AllowanceIssueDiagnostics
    private let logger: SDKLoggerProtocol
    private let claimCoordinator = ClaimCoordinator()

    public init(
        infoProvider: BulletInSlotInfoProviding,
        allocator: AllowanceSlotAllocating,
        backgroundExecutor: any BackgroundExecuting,
        issueDiagnostics: AllowanceIssueDiagnostics,
        logger: SDKLoggerProtocol
    ) {
        self.infoProvider = infoProvider
        self.allocator = allocator
        self.backgroundExecutor = backgroundExecutor
        self.issueDiagnostics = issueDiagnostics
        self.logger = logger
    }
}

extension BulletInAllowanceManager: AllowanceManaging {
    static let timeout: Duration = .seconds(60)
    // Overall bound for one coalesced check-and-claim, so a stalled claim can never keep the
    // in-flight task (and every upload coalesced onto it) waiting forever.
    static let claimTimeout: Duration = .seconds(120)

    public func allocate(
        accountId: AccountId,
        policy: OnExistingAllowancePolicy,
        priority: AllowanceRecord.Priority
    ) async throws {
        try await issueDiagnostics.observeAllocation {
            try await allocateInBackground(accountId: accountId, policy: policy, priority: priority)
        }
    }

    /// Gates on `HopRuntimeApi.can_account_promote` instead of the stored allowance extent: HOP
    /// requires an unexpired authorization; the remaining extent is not checked. Products keep using
    /// [allocate], which reads the extent to honour capacity.
    public func ensureCanSubmit(
        accountId: AccountId,
        priority: AllowanceRecord.Priority
    ) async throws {
        // Coalesce overlapping calls for the same account: a second upload started while the first is
        // still claiming shares that in-flight check-and-claim instead of claiming a redundant slot.
        try await claimCoordinator.coalesce(for: accountId) {
            try await self.issueDiagnostics.observeAllocation {
                try await withTimeout(Self.claimTimeout) {
                    try await self.ensureCanSubmitInBackground(accountId: accountId, priority: priority)
                }
            }
        }
    }
}

private extension BulletInAllowanceManager {
    func allocateInBackground(
        accountId: AccountId,
        policy: OnExistingAllowancePolicy,
        priority: AllowanceRecord.Priority
    ) async throws {
        try await backgroundExecutor.execute { [allocator, infoProvider] in
            try await markStallActivity("Allocating Bulletin allowance") {
                let currentAllowance = try await markStallRegion("Check existing allowance") {
                    try await infoProvider.fetchAllowance(for: accountId)
                }

                if let currentAllowance, currentAllowance.available, policy == .ignore {
                    return
                }

                try await allocator.assignSlot(accountId: accountId, priority: priority)

                try await infoProvider.waitAuthorization(
                    for: accountId,
                    currentAllowance: currentAllowance,
                    timeout: Self.timeout
                )
            }
        }
    }

    func ensureCanSubmitInBackground(accountId: AccountId, priority: AllowanceRecord.Priority) async throws {
        try await backgroundExecutor.execute { [allocator, infoProvider, logger] in
            try await markStallActivity("Ensuring Bulletin allowance") {
                let submittable = try await markStallRegion("Check submit eligibility") {
                    try await infoProvider.canAccountPromote(for: accountId)
                }

                guard !submittable else {
                    logger.debug("HOP: can_account_promote=true, skipping claim")
                    return
                }

                logger.debug("HOP: can_account_promote=false, claiming long-term storage slot")
                try await allocator.assignSlot(accountId: accountId, priority: priority)

                try await infoProvider.waitSubmittable(for: accountId, timeout: Self.timeout)
                logger.debug("HOP: authorization active on Bulletin after claim")
            }
        }
    }
}

/// Serializes the submit check-and-claim per account so concurrent uploads coalesce onto one
/// in-flight operation instead of each submitting a redundant `claim_long_term_storage`.
private actor ClaimCoordinator {
    private var inFlight: [AccountId: Task<Void, Error>] = [:]

    func coalesce(
        for accountId: AccountId,
        operation: @Sendable @escaping () async throws -> Void
    ) async throws {
        if let existing = inFlight[accountId] {
            return try await existing.value
        }

        let task = Task { try await operation() }
        inFlight[accountId] = task
        defer { inFlight[accountId] = nil }

        try await task.value
    }
}
