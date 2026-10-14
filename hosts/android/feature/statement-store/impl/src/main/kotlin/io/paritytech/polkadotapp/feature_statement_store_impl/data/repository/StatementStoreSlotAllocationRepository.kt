@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_statement_store_impl.data.repository

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.SlotPriority
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * One persistent record of an on-chain slot owned by this device.
 *
 * @param id Room-generated primary key; used by the repository to update/delete.
 * @param accountId Owner of the slot.
 * @param seq The on-chain seq this row currently claims, scoped to [latestRenewedPeriod].
 *   Updated whenever the renewer migrates the row to a new period.
 * @param latestRenewedPeriod Last period this row was renewed (or first allocated) in.
 * @param since Local clock at last allocate/renew; LRU ordering key.
 * @param priority The priority the caller specified at allocate time.
 */
class StatementStoreSlotAllocationRecord(
    val id: Long,
    val accountId: AccountId,
    val seq: UInt,
    val latestRenewedPeriod: UInt,
    val since: Instant,
    val priority: SlotPriority,
)

interface StatementStoreSlotAllocationRepository {
    /**
     * Inserts a new row for a freshly-allocated slot. Returns the generated id.
     */
    suspend fun insert(
        chainId: ChainId,
        collection: PeopleCollection,
        accountId: AccountId,
        seq: UInt,
        latestRenewedPeriod: UInt,
        since: Instant,
        priority: SlotPriority,
    ): Long

    suspend fun deleteById(id: Long)

    /**
     * Deletes the row matching the exact on-chain `(account, seq)` coordinate that the
     * allocator just evicted. No-op when no row matches (the slot was external to this
     * device).
     */
    suspend fun deleteSlot(
        chainId: ChainId,
        collection: PeopleCollection,
        accountId: AccountId,
        seq: UInt,
    )

    /**
     * Drops every row owned by [accountId] on [chainId], across all collections and periods.
     * Local accounting only — any slot the account still holds on-chain is left to expire.
     */
    suspend fun deleteAllForAccount(chainId: ChainId, accountId: AccountId)

    /**
     * All rows on [chainId] whose `latestRenewedPeriod` is strictly less than
     * [currentPeriod], regardless of collection — renewal can migrate a row into any
     * available collection, so staleness is not scoped by collection.
     */
    suspend fun staleRows(
        chainId: ChainId,
        currentPeriod: UInt,
    ): List<StatementStoreSlotAllocationRecord>

    suspend fun hasStaleFor(
        chainId: ChainId,
        accountId: AccountId,
        currentPeriod: UInt,
    ): Boolean

    /**
     * Bulk lookup of "highest local priority" per account in [accountIds], across all
     * collections. The allocator uses this once per `pickSeq` call to project effective
     * priorities for every eviction candidate in one query rather than N. Accounts with
     * no local rows are absent from the returned map.
     */
    suspend fun maxPriorityLevelsFor(
        chainId: ChainId,
        accountIds: Collection<AccountId>,
    ): Map<AccountId, SlotPriority>

    /**
     * Records that the row at [id] just got renewed into a new period at on-chain
     * coordinate [newSeq] in [newCollection], with a fresh [since] for LRU.
     */
    suspend fun markRenewed(id: Long, period: UInt, since: Instant, newSeq: UInt, newCollection: PeopleCollection)
}
