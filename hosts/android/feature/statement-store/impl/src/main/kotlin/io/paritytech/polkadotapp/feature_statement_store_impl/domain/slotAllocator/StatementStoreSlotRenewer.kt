package io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator

import io.paritytech.polkadotapp.common.domain.model.AccountId

interface StatementStoreSlotRenewer {
    /**
     * Full sync: every stale row for the current `(chain, collection)` is considered.
     * Rows are sorted `(priority.level DESC, accountId == priorityAccount ? 0 : 1,
     * sinceMillis DESC)` and paired with free seqs in that order. Rows that exceed
     * the free-seq pool are deleted unconditionally. The chain capacity is the truth.
     *
     * When [priorityAccount] is `null` the sort collapses to pure LRU-by-priority —
     * this is the periodic worker's mode. Allocator force-launches the renewer with
     * a target account so its rows win the available seqs when capacity is tight.
     */
    suspend fun renew(context: AllocateContext, priorityAccount: AccountId? = null): Result<Unit>
}
