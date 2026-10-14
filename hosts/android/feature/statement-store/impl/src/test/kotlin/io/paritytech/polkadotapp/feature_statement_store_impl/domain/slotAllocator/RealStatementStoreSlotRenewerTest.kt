@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator

import io.paritytech.polkadotapp.chains.extrinsic.ExtrinsicStatus
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.CurrentTimeContext
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.SlotPriority
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementSlotsForCollection
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlot
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlots
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.StatementStoreSlotAllocationRecord
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.StatementStoreSlotAllocationRepository
import io.paritytech.polkadotapp.feature_statement_store_impl.data.signer.origins.StatementStoreOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.anyLong
import io.paritytech.polkadotapp.test_shared.anyUInt
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.eqUInt
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.times
import org.mockito.Mockito.verify
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class RealStatementStoreSlotRenewerTest {
    private val chainId = "people"
    private val collection = PeopleCollection.People
    private val liteCollection = PeopleCollection.LitePeople
    private val period = 100u

    private val accountA: AccountId = byteArrayOf(0xA).toDataByteArray()
    private val accountB: AccountId = byteArrayOf(0xB).toDataByteArray()

    private val chain: Chain = mock(Chain::class.java)
    private val allocationRepository: StatementStoreSlotAllocationRepository =
        mock(StatementStoreSlotAllocationRepository::class.java)
    private val slotLoader: StatementStoreSlotLoader = mock(StatementStoreSlotLoader::class.java)
    private val origins: StatementStoreOrigins = mock(StatementStoreOrigins::class.java)
    private val extrinsicService: ExtrinsicService = mock(ExtrinsicService::class.java)

    private val fixedNow: Instant = Instant.fromEpochSeconds(1_000_000)

    private val renewer = RealStatementStoreSlotRenewer(
        allocationRepository = allocationRepository,
        slotLoader = slotLoader,
        statementStoreOrigins = origins,
        extrinsicService = extrinsicService,
        currentTimeContext = CurrentTimeContext { fixedNow },
    )

    private val context = AllocateContext(chain, listOf(collection, liteCollection), period)

    @Before
    fun setUp() = runBlocking<Unit> {
        whenever(chain.id).thenReturn(chainId)
    }

    // -- Comparator (pure-logic) tests --

    @Test
    fun `comparator - no priority - sort is pure LRU by sinceMillis DESC within same level`() {
        val rows = listOf(
            row(id = 1, accountId = accountA, since = 100, priority = SlotPriority.Normal),
            row(id = 2, accountId = accountB, since = 300, priority = SlotPriority.Normal),
            row(id = 3, accountId = accountA, since = 200, priority = SlotPriority.Normal),
        )

        val sortedIds = rows.sortedWith(renewalComparator(priorityAccount = null)).map { it.id }

        assertEquals(listOf<Long>(2, 3, 1), sortedIds)
    }

    @Test
    fun `comparator - higher level comes before lower regardless of sinceMillis`() {
        val rows = listOf(
            row(id = 1, accountId = accountA, since = 999, priority = SlotPriority.Normal),
            row(id = 2, accountId = accountA, since = 100, priority = SlotPriority.High),
        )

        val sortedIds = rows.sortedWith(renewalComparator(priorityAccount = null)).map { it.id }

        assertEquals(listOf<Long>(2, 1), sortedIds)
    }

    @Test
    fun `comparator - priority account wins ties within same level by going first`() {
        val rows = listOf(
            row(id = 1, accountId = accountA, since = 100, priority = SlotPriority.High),
            row(id = 2, accountId = accountB, since = 500, priority = SlotPriority.High),
            row(id = 3, accountId = accountA, since = 50, priority = SlotPriority.High),
        )

        val sortedIds = rows.sortedWith(renewalComparator(priorityAccount = accountA)).map { it.id }

        // Same level; A first by since DESC: [1, 3]; then B: [2].
        assertEquals(listOf<Long>(1, 3, 2), sortedIds)
    }

    // -- End-to-end renewer tests --

    @Test
    fun `renewer - no priority - full capacity - mixed results - mark successes, delete failures`() = runBlocking<Unit> {
        val rows = listOf(
            row(id = 10, accountId = accountA, since = 300, priority = SlotPriority.Normal),
            row(id = 20, accountId = accountA, since = 200, priority = SlotPriority.Normal),
            row(id = 30, accountId = accountA, since = 100, priority = SlotPriority.Normal),
        )
        // Sort (no priority): id 10 (since=300), 20 (200), 30 (100). 3 free seqs.
        withStaleRows(rows)
        withFreeSeqs(listOf(0u, 1u, 2u))
        withBatchResults(listOf(success(), failure(RuntimeException("rejected")), success()))

        val result = renewer.renew(context, priorityAccount = null)

        assertSuccess(result)
        verifyMarkedRenewed(id = 10)
        verifyDeletedById(id = 20)
        verifyMarkedRenewed(id = 30)
    }

    @Test
    fun `renewer - no priority - overflow - newest renewed, oldest deleted`() = runBlocking<Unit> {
        val rows = listOf(
            row(id = 10, accountId = accountA, since = 100, priority = SlotPriority.Normal), // oldest
            row(id = 20, accountId = accountA, since = 200, priority = SlotPriority.Normal),
            row(id = 30, accountId = accountA, since = 300, priority = SlotPriority.Normal), // newest
        )
        withStaleRows(rows)
        withFreeSeqs(listOf(0u))
        withBatchResults(listOf(success()))

        val result = renewer.renew(context, priorityAccount = null)

        assertSuccess(result)
        verifyMarkedRenewed(id = 30)
        verifyDeletedById(id = 10)
        verifyDeletedById(id = 20)
    }

    @Test
    fun `renewer - with priority - overflow - target's older row beats other's newer row`() = runBlocking<Unit> {
        val rows = listOf(
            row(id = 10, accountId = accountA, since = 100, priority = SlotPriority.Normal), // t_old
            row(id = 20, accountId = accountB, since = 500, priority = SlotPriority.Normal), // b_newest
        )
        withStaleRows(rows)
        withFreeSeqs(listOf(0u))
        withBatchResults(listOf(success()))

        val result = renewer.renew(context, priorityAccount = accountA)

        assertSuccess(result)
        verifyMarkedRenewed(id = 10)
        verifyDeletedById(id = 20)
    }

    @Test
    fun `renewer - with priority - ample capacity - all renewed regardless of account`() = runBlocking<Unit> {
        val rows = listOf(
            row(id = 10, accountId = accountA, since = 100, priority = SlotPriority.Normal),
            row(id = 20, accountId = accountB, since = 200, priority = SlotPriority.Normal),
        )
        withStaleRows(rows)
        withFreeSeqs(listOf(0u, 1u, 2u))
        withBatchResults(listOf(success(), success()))

        val result = renewer.renew(context, priorityAccount = accountA)

        assertSuccess(result)
        verifyMarkedRenewed(id = 10)
        verifyMarkedRenewed(id = 20)
        verify(allocationRepository, never()).deleteById(anyLong())
    }

    @Test
    fun `renewer - submit failure - failing row deleted, succeeding row renewed`() = runBlocking<Unit> {
        val rows = listOf(
            row(id = 10, accountId = accountA, since = 200, priority = SlotPriority.Normal),
            row(id = 20, accountId = accountA, since = 100, priority = SlotPriority.Normal),
        )
        withStaleRows(rows)
        withFreeSeqs(listOf(0u, 1u))
        withBatchResults(listOf(success(), failure(RuntimeException("on-chain reject"))))

        val result = renewer.renew(context, priorityAccount = null)

        assertSuccess(result)
        verifyMarkedRenewed(id = 10)
        verifyDeletedById(id = 20)
    }

    @Test
    fun `renewer - no stale rows - skips load and submit`() = runBlocking<Unit> {
        withStaleRows(emptyList())

        val result = renewer.renew(context, priorityAccount = null)

        assertSuccess(result)
        verify(allocationRepository, never()).markRenewed(anyLong(), anyUInt(), any(), anyUInt(), any())
        verify(allocationRepository, never()).deleteById(anyLong())
    }

    @Test
    fun `renewer - composite sort with priority + priority-account - high target beats normal target newest`() = runBlocking<Unit> {
        val rows = listOf(
            row(id = 10, accountId = accountA, since = 100, priority = SlotPriority.High), // t_High (oldest)
            row(id = 20, accountId = accountA, since = 500, priority = SlotPriority.Normal), // t_Normal (newest of A)
            row(id = 30, accountId = accountB, since = 500, priority = SlotPriority.High), // b_High
        )
        withStaleRows(rows)
        withFreeSeqs(listOf(0u))
        withBatchResults(listOf(success()))

        val result = renewer.renew(context, priorityAccount = accountA)

        assertSuccess(result)
        // priority DESC, accountMatch, sinceMillis DESC → [10 (High+target), 30 (High+other), 20 (Normal+target)].
        // 1 free seq: id 10 wins, 30 and 20 are overflow.
        verifyMarkedRenewed(id = 10)
        verifyDeletedById(id = 20)
        verifyDeletedById(id = 30)
    }

    // -- multi-collection tests --

    @Test
    fun `renewer - migrates lite row into People seq when lite has no free seq`() = runBlocking<Unit> {
        withStaleRows(
            listOf(row(id = 10, accountId = accountA, since = 100, priority = SlotPriority.Normal))
        )
        withSlots(
            StatementSlotsForCollection(collection, listOf(StatementStoreSlot.Free(5u))),
            StatementSlotsForCollection(liteCollection, emptyList()),
        )
        withBatchResults(listOf(success()))

        val result = renewer.renew(context, priorityAccount = null)

        assertSuccess(result)
        verifyMarkedRenewedInto(id = 10, seq = 5u, collection = collection)
    }

    @Test
    fun `renewer - renews stale rows from both collections in one pass`() = runBlocking<Unit> {
        withStaleRows(
            listOf(
                row(id = 10, accountId = accountA, since = 200, priority = SlotPriority.Normal),
                row(id = 20, accountId = accountB, since = 100, priority = SlotPriority.Normal),
            )
        )
        withSlots(
            StatementSlotsForCollection(collection, listOf(StatementStoreSlot.Free(0u))),
            StatementSlotsForCollection(liteCollection, listOf(StatementStoreSlot.Free(1u))),
        )
        withBatchResults(listOf(success(), success()))

        val result = renewer.renew(context, priorityAccount = null)

        assertSuccess(result)
        // sorted by since DESC, zipped with pooled free seqs [People:0, LitePeople:1].
        verifyMarkedRenewedInto(id = 10, seq = 0u, collection = collection)
        verifyMarkedRenewedInto(id = 20, seq = 1u, collection = liteCollection)
    }

    @Test
    fun `renewer - global priority pooling - High lite row beats Normal people row for scarce seq`() = runBlocking<Unit> {
        withStaleRows(
            listOf(
                row(id = 10, accountId = accountA, since = 100, priority = SlotPriority.High),
                row(id = 20, accountId = accountB, since = 500, priority = SlotPriority.Normal),
            )
        )
        withSlots(
            StatementSlotsForCollection(collection, listOf(StatementStoreSlot.Free(0u))),
            StatementSlotsForCollection(liteCollection, emptyList()),
        )
        withBatchResults(listOf(success()))

        val result = renewer.renew(context, priorityAccount = null)

        assertSuccess(result)
        // High row wins the only seq (migrating lite -> People); Normal row overflows.
        verifyMarkedRenewedInto(id = 10, seq = 0u, collection = collection)
        verifyDeletedById(id = 20)
    }

    @Test
    fun `renewer - overflow counted against summed pool across collections`() = runBlocking<Unit> {
        withStaleRows(
            listOf(
                row(id = 10, accountId = accountA, since = 300, priority = SlotPriority.Normal),
                row(id = 20, accountId = accountA, since = 200, priority = SlotPriority.Normal),
                row(id = 30, accountId = accountA, since = 100, priority = SlotPriority.Normal),
            )
        )
        // Capacity is the SUM of free seqs across collections: People:0 + LitePeople:1 = 2.
        withSlots(
            StatementSlotsForCollection(collection, listOf(StatementStoreSlot.Free(0u))),
            StatementSlotsForCollection(liteCollection, listOf(StatementStoreSlot.Free(1u))),
        )
        withBatchResults(listOf(success(), success()))

        val result = renewer.renew(context, priorityAccount = null)

        assertSuccess(result)
        // sorted by since DESC, zipped with pooled free seqs [People:0, LitePeople:1]:
        // 10 and 20 renew (one per collection); 30 overflows.
        verifyMarkedRenewedInto(id = 10, seq = 0u, collection = collection)
        verifyMarkedRenewedInto(id = 20, seq = 1u, collection = liteCollection)
        verifyDeletedById(id = 30)
    }

    // -- setup helpers (every used method stubbed explicitly) --

    private suspend fun withStaleRows(rows: List<StatementStoreSlotAllocationRecord>) {
        whenever(allocationRepository.staleRows(eq(chainId), anyUInt())).thenReturn(rows)
    }

    private suspend fun withFreeSeqs(seqs: List<UInt>) {
        withSlots(StatementSlotsForCollection(collection, seqs.map { StatementStoreSlot.Free(it) }))
    }

    private suspend fun withSlots(vararg perCollection: StatementSlotsForCollection) {
        val slots = StatementStoreSlots(perCollection.toList())
        whenever(slotLoader.loadSlots(any())).thenReturn(Result.success(slots))
        whenever(origins.asResourcesStatementStoreSlot(anyUInt(), anyUInt(), any())).thenReturn(Result.success(mock()))
    }

    private suspend fun withBatchResults(results: List<Result<ExtrinsicStatus.InBlock>>) {
        whenever(extrinsicService.submitExtrinsicsAndAwaitInBlock(any(), any(), any(), any()))
            .thenReturn(Result.success(results))
    }

    // -- verification helpers --

    private fun assertSuccess(result: Result<*>) {
        assertTrue("expected Result.success but was ${result.exceptionOrNull()}", result.isSuccess)
    }

    private suspend fun verifyMarkedRenewed(id: Long) {
        verify(allocationRepository, times(1)).markRenewed(eq(id), anyUInt(), any(), anyUInt(), any())
    }

    private suspend fun verifyMarkedRenewedInto(id: Long, seq: UInt, collection: PeopleCollection) {
        verify(allocationRepository, times(1)).markRenewed(eq(id), anyUInt(), any(), eqUInt(seq), eq(collection))
    }

    private suspend fun verifyDeletedById(id: Long) {
        verify(allocationRepository, times(1)).deleteById(eq(id))
    }

    // -- fixtures --

    private fun row(
        id: Long,
        accountId: AccountId,
        since: Long,
        priority: SlotPriority,
    ) = StatementStoreSlotAllocationRecord(
        id = id,
        accountId = accountId,
        seq = 0u,
        latestRenewedPeriod = period - 1u,
        since = Instant.fromEpochSeconds(since),
        priority = priority,
    )

    private fun success(): Result<ExtrinsicStatus.InBlock> =
        Result.success(ExtrinsicStatus.InBlock(blockHash = "0x00", extrinsicHash = "0x00"))

    private fun failure(throwable: Throwable): Result<ExtrinsicStatus.InBlock> =
        Result.failure(throwable)

    private inline fun <reified T : Any> mock(): T = mock(T::class.java)
}
