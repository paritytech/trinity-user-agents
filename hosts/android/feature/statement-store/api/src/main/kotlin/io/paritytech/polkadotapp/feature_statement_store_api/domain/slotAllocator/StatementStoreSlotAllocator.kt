package io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector

interface StatementStoreSlotAllocator {
    /**
     * Allocates a statement store slot for [target] in the current period.
     *
     * When [strategy] is [OnExistingAllocationStrategy.IGNORE] and [target] already
     * holds a slot in this period the call is a no-op.
     *
     * Otherwise, picks the first free seq; if the slot table is full, LRU-evicts the
     * oldest slot not owned by [target] whose `StmtStoreReplacementCooldown` has
     * elapsed and whose effective priority is `<= priority.level`. Fails with
     * [StatementStoreSlotAllocationError.NoAllocationAvailable] if neither a free
     * nor an evictable slot exists.
     *
     * [priority] is also persisted on the local accounting row so renewal across
     * period rollovers can preserve higher-tier claims under capacity pressure.
     *
     * Reports its progress into [diagnostics]; callers with no UI attached pass
     * [StalenessReportCollector.NoOp].
     */
    context(diagnostics: StalenessReportCollector)
    suspend fun allocate(
        target: AccountId,
        strategy: OnExistingAllocationStrategy,
        priority: SlotPriority,
    ): Result<Unit>

    /**
     * Drops every local slot-accounting row owned by [target], so renewal stops treating
     * its slots as ours and they become ordinary eviction candidates once the current
     * period ends. Purely local — nothing is released on-chain.
     */
    suspend fun deallocateAllSlots(target: AccountId): Result<Unit>

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
