package io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector

interface StatementStoreSlotAllocator {
    /** The period slots are allocated in right now. */
    fun currentPeriod(): UInt

    /**
     * Allocates a statement store slot for [target] in the current period.
     *
     * When [strategy] is [OnExistingAllocationStrategy.IGNORE] and [target] already
     * holds a slot in this period the call is a no-op.
     *
     * Otherwise, first renews every stale local row, then picks the first free seq; if the
     * slot table is full, LRU-evicts the oldest slot not owned by [target] whose
     * `StmtStoreReplacementCooldown` has elapsed and whose effective priority can be evicted
     * by [priority] (see [canBeEvictedBy]). Fails with
     * [StatementStoreSlotAllocationError.NoAllocationAvailable] if neither a free
     * nor an evictable slot exists.
     *
     * [priority] is also persisted on the local accounting row so renewal across
     * period rollovers can preserve higher-tier claims under capacity pressure. So is [tag],
     * which lets its owner find the row again with [allocatedTargets].
     *
     * Reports its progress into [diagnostics]; callers with no UI attached pass
     * [StalenessReportCollector.NoOp].
     */
    context(diagnostics: StalenessReportCollector)
    suspend fun allocate(
        target: AccountId,
        strategy: OnExistingAllocationStrategy,
        priority: SlotPriority,
        tag: StatementStoreSlotTag? = null,
    ): Result<Unit>

    /**
     * Drops every local slot-accounting row owned by [target], so renewal stops treating
     * its slots as ours and they become ordinary eviction candidates once the current
     * period ends. Purely local — nothing is released on-chain.
     */
    suspend fun deallocateAllSlots(target: AccountId): Result<Unit>

    /**
     * Local-only check: whether [target] has a slot-accounting row allocated or renewed in the
     * current period. Optimistic — does not consult the chain, so an on-chain eviction by another
     * device is not reflected.
     */
    suspend fun hasCurrentAllocation(target: AccountId): Result<Boolean>

    /**
     * Local-only: accounts holding a slot-accounting row allocated with [tag], so its owner can drop rows it no
     * longer needs even when it lost track of them (e.g. a crash between its own state change and
     * [deallocateAllSlots]).
     */
    suspend fun allocatedTargets(tag: StatementStoreSlotTag): Result<Set<AccountId>>

    /**
     * Snapshot of all slots taken in the current period across the available collections,
     * resolved in the same context that [allocate] would use for [target].
     */
    suspend fun allocationsFor(target: AccountId): Result<StatementStoreSlots>

    /**
     * Enqueues the periodic renewer that re-allocates slots into each new period.
     * Fire-and-forget — returns success once enqueue is acknowledged by WorkManager.
     * Called once on app start from `RootViewModel`.
     */
    suspend fun scheduleSlotRenewals(): Result<Unit>
}

sealed class StatementStoreSlotAllocationError(cause: Throwable?) : Throwable(cause) {
    class NoAllocationAvailable(cause: Throwable) : StatementStoreSlotAllocationError(cause)
    class Unknown(cause: Throwable) : StatementStoreSlotAllocationError(cause)
}
