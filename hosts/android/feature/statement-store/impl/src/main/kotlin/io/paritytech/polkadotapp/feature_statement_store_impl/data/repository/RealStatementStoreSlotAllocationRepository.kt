@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_statement_store_impl.data.repository

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.database.dao.StatementStoreSlotAllocationDao
import io.paritytech.polkadotapp.database.model.StatementStoreSlotAllocationLocal
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.SlotPriority
import javax.inject.Inject
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class RealStatementStoreSlotAllocationRepository @Inject constructor(
    private val dao: StatementStoreSlotAllocationDao,
) : StatementStoreSlotAllocationRepository {
    override suspend fun insert(
        chainId: ChainId,
        collection: PeopleCollection,
        accountId: AccountId,
        seq: UInt,
        latestRenewedPeriod: UInt,
        since: Instant,
        priority: SlotPriority,
    ): Long {
        val row = StatementStoreSlotAllocationLocal(
            id = 0,
            chainId = chainId,
            collection = collection.name,
            accountId = accountId.value,
            seq = seq.toInt(),
            latestRenewedPeriod = latestRenewedPeriod.toLong(),
            sinceMillis = since.toEpochMilliseconds(),
            priorityLevel = priority.level,
        )
        return dao.insert(row)
    }

    override suspend fun deleteById(id: Long) {
        dao.deleteById(id)
    }

    override suspend fun deleteSlot(
        chainId: ChainId,
        collection: PeopleCollection,
        accountId: AccountId,
        seq: UInt,
    ) {
        dao.deleteSlot(chainId, collection.name, accountId.value, seq.toInt())
    }

    override suspend fun deleteAllForAccount(chainId: ChainId, accountId: AccountId) {
        dao.deleteAllForAccount(chainId, accountId.value)
    }

    override suspend fun staleRows(
        chainId: ChainId,
        currentPeriod: UInt,
    ): List<StatementStoreSlotAllocationRecord> {
        return dao.staleRows(chainId, currentPeriod.toLong())
            .map { it.toRecord() }
    }

    override suspend fun hasStaleFor(
        chainId: ChainId,
        accountId: AccountId,
        currentPeriod: UInt,
    ): Boolean {
        return dao.hasStaleFor(chainId, accountId.value, currentPeriod.toLong())
    }

    override suspend fun maxPriorityLevelsFor(
        chainId: ChainId,
        accountIds: Collection<AccountId>,
    ): Map<AccountId, SlotPriority> {
        if (accountIds.isEmpty()) return emptyMap()

        val rows = dao.maxPriorityLevelsFor(chainId, accountIds.map { it.value })

        return rows.associateBy(
            keySelector = { it.accountId.toDataByteArray() },
            valueTransform = { priorityFromLevel(it.maxPriorityLevel) },
        )
    }

    override suspend fun markRenewed(id: Long, period: UInt, since: Instant, newSeq: UInt, newCollection: PeopleCollection) {
        dao.markRenewed(id, period.toLong(), since.toEpochMilliseconds(), newSeq.toInt(), newCollection.name)
    }

    private fun StatementStoreSlotAllocationLocal.toRecord() = StatementStoreSlotAllocationRecord(
        id = id,
        accountId = accountId.toDataByteArray(),
        seq = seq.toUInt(),
        latestRenewedPeriod = latestRenewedPeriod.toUInt(),
        since = Instant.fromEpochMilliseconds(sinceMillis),
        priority = priorityFromLevel(priorityLevel),
    )

    private fun priorityFromLevel(level: Int): SlotPriority =
        SlotPriority.entries.firstOrNull { it.level == level }
            ?: error("Unknown SlotPriority level $level in statement_store_slot_allocations; was it inserted by a newer build?")
}
