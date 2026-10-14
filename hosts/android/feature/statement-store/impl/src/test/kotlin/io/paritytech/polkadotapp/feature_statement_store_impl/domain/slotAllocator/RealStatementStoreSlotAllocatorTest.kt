@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator

import android.content.Context
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.CurrentTimeContext
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.OnExistingAllocationStrategy
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.SlotPriority
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementSlotsForCollection
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlot
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotAllocationError
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlots
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.StatementStoreSlotAllocationRepository
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.StatementStoreSlotRepository
import io.paritytech.polkadotapp.feature_statement_store_impl.data.signer.origins.StatementStoreOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicDispatch
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicExecutionResult
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.anyUInt
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.eqUInt
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.times
import org.mockito.Mockito.verify
import kotlin.time.Duration.Companion.seconds
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class RealStatementStoreSlotAllocatorTest {
    private val chainId: ChainId = "people"
    private val collection = PeopleCollection.People
    private val liteCollection = PeopleCollection.LitePeople
    private val period = 100u

    private val target: AccountId = byteArrayOf(0x01).toDataByteArray()
    private val accountHigh: AccountId = byteArrayOf(0x02).toDataByteArray()
    private val accountNormal: AccountId = byteArrayOf(0x03).toDataByteArray()
    private val accountUntracked: AccountId = byteArrayOf(0x04).toDataByteArray()

    private val fixedNow: Instant = Instant.fromEpochSeconds(1_000_000)
    private val fixedTimeContext = CurrentTimeContext { fixedNow }

    private val appContext: Context = mock(Context::class.java)
    private val chain: Chain = mock(Chain::class.java)

    private val slotRepository: StatementStoreSlotRepository = mock(StatementStoreSlotRepository::class.java)
    private val origins: StatementStoreOrigins = mock(StatementStoreOrigins::class.java)
    private val extrinsicService: ExtrinsicService = mock(ExtrinsicService::class.java)
    private val slotLoader: StatementStoreSlotLoader = mock(StatementStoreSlotLoader::class.java)
    private val contextResolver: AllocateContextResolver = mock(AllocateContextResolver::class.java)
    private val allocationRepository: StatementStoreSlotAllocationRepository =
        mock(StatementStoreSlotAllocationRepository::class.java)
    private val renewer: StatementStoreSlotRenewer = mock(StatementStoreSlotRenewer::class.java)
    private val lock = StatementStoreSlotRenewalLock()
    private val knownChains = KnownChains(people = chainId, assetHub = "assetHub", bulletIn = "bulletIn", hydration = null)

    private val allocator = RealStatementStoreSlotAllocator(
        appContext = appContext,
        extrinsicService = extrinsicService,
        statementStoreOrigins = origins,
        statementStoreSlotRepository = slotRepository,
        slotLoader = slotLoader,
        contextResolver = contextResolver,
        knownChains = knownChains,
        allocationRepository = allocationRepository,
        renewer = renewer,
        renewalLock = lock,
        currentTimeContext = fixedTimeContext,
    )

    @Before
    fun setUp() = runBlocking<Unit> {
        // Only the truly universal stubs. Each test states its stale-vs-fresh assumption.
        withChainContext()
        withRenewerSucceeds()
        withNoTrackedPriorities()
    }

    @Test
    fun `IGNORE no-op when target already has a slot - no submit, no DB writes`() = runBlocking<Unit> {
        withNoStaleAllocations()
        withSlotTakenBy(target)

        val result = allocate(OnExistingAllocationStrategy.IGNORE, SlotPriority.Normal)

        assertSuccess(result)
        verifyNoAllocationInsertions()
        verifyNoEvictionPerformed()
        verifyNoExtrinsicSubmitted()
        assertRenewerNotInvoked()
    }

    @Test
    fun `target-priority invocation - stale rows for target trigger renewer with priorityAccount = target`() = runBlocking<Unit> {
        withStaleAllocationsFor(target)
        withSlotTakenBy(target)

        allocate(OnExistingAllocationStrategy.IGNORE, SlotPriority.Normal)

        assertRenewerInvokedWith(priorityAccount = target)
    }

    @Test
    fun `no renewer call when target rows are fresh`() = runBlocking<Unit> {
        withNoStaleAllocations()
        withSlotTakenBy(target)

        allocate(OnExistingAllocationStrategy.IGNORE, SlotPriority.Normal)

        assertRenewerNotInvoked()
    }

    @Test
    fun `Normal caller cannot evict High-priority slot - NoAllocationAvailable`() = runBlocking<Unit> {
        withNoStaleAllocations()
        withZeroCooldown()
        withSlotTakenBy(accountHigh)
        withTrackedPriorities(accountHigh to SlotPriority.High)

        val result = allocate(OnExistingAllocationStrategy.INCREASE, SlotPriority.Normal)

        assertNoAllocationAvailable(result)
        verifyNoAllocationInsertions()
        verifyNoEvictionPerformed()
        verifyNoExtrinsicSubmitted()
    }

    @Test
    fun `Normal caller evicts Normal slot - delete oldest + insert target row`() = runBlocking<Unit> {
        withNoStaleAllocations()
        withZeroCooldown()
        withSlotTakenBy(accountNormal)
        withTrackedPriorities(accountNormal to SlotPriority.Normal)
        withSuccessfulSubmission()

        val result = allocate(OnExistingAllocationStrategy.INCREASE, SlotPriority.Normal)

        assertSuccess(result)
        verifySlotDeletedFor(accountNormal, seq = 0u)
        verifyAllocationInsertedFor(target, seq = 0u, priority = SlotPriority.Normal)
    }

    @Test
    fun `High caller evicts High slot`() = runBlocking<Unit> {
        withNoStaleAllocations()
        withZeroCooldown()
        withSlotTakenBy(accountHigh)
        withTrackedPriorities(accountHigh to SlotPriority.High)
        withSuccessfulSubmission()

        val result = allocate(OnExistingAllocationStrategy.INCREASE, SlotPriority.High)

        assertSuccess(result)
        verifySlotDeletedFor(accountHigh, seq = 0u)
        verifyAllocationInsertedFor(target, seq = 0u, priority = SlotPriority.High)
    }

    @Test
    fun `High caller prefers evicting Normal slot over older High slot when both are evictable`() = runBlocking<Unit> {
        withNoStaleAllocations()
        withZeroCooldown()
        withSlotsTakenBy(accountHigh, accountNormal) // seq 0 (older) = High, seq 1 (newer) = Normal
        withTrackedPriorities(
            accountHigh to SlotPriority.High,
            accountNormal to SlotPriority.Normal,
        )
        withSuccessfulSubmission()

        val result = allocate(OnExistingAllocationStrategy.INCREASE, SlotPriority.High)

        assertSuccess(result)
        verifySlotDeletedFor(accountNormal, seq = 1u)
        verifyNoEvictionOf(accountHigh)
    }

    @Test
    fun `Untracked on-chain slot defaults to Normal for eviction filter`() = runBlocking<Unit> {
        withNoStaleAllocations()
        withZeroCooldown()
        withSlotTakenBy(accountUntracked)
        // No tracked priority for accountUntracked → bulk map returns empty → allocator treats as Normal.
        withSuccessfulSubmission()

        val result = allocate(OnExistingAllocationStrategy.INCREASE, SlotPriority.Normal)

        assertSuccess(result)
        verifySlotDeletedFor(accountUntracked, seq = 0u)
    }

    @Test
    fun `free seq path - no eviction, just insert target row with caller priority`() = runBlocking<Unit> {
        withNoStaleAllocations()
        withFreeSlot()
        withSuccessfulSubmission()

        val result = allocate(OnExistingAllocationStrategy.INCREASE, SlotPriority.High)

        assertSuccess(result)
        verifyNoEvictionPerformed()
        verifyAllocationInsertedFor(target, seq = 0u, priority = SlotPriority.High)
    }

    @Test
    fun `deallocateAllSlots drops every local row of the target without touching the chain`() = runBlocking<Unit> {
        val result = allocator.deallocateAllSlots(target)

        assertSuccess(result)
        verify(allocationRepository, times(1)).deleteAllForAccount(eq(chainId), eq(target))
        verifyNoExtrinsicSubmitted()
    }

    // -- multi-collection tests --

    @Test
    fun `pooled free seq - People full, LitePeople free - new slot lands in LitePeople`() = runBlocking<Unit> {
        withAvailableCollections(collection, liteCollection)
        withNoStaleAllocations()
        withChainSlots(
            StatementSlotsForCollection(collection, takenSlots(accountNormal)),
            StatementSlotsForCollection(liteCollection, listOf(StatementStoreSlot.Free(seq = 0u))),
        )
        withSuccessfulSubmission()

        val result = allocate(OnExistingAllocationStrategy.INCREASE, SlotPriority.High)

        assertSuccess(result)
        verifyNoEvictionPerformed()
        verifyAllocationInsertedFor(target, seq = 0u, priority = SlotPriority.High, inCollection = liteCollection)
    }

    @Test
    fun `IGNORE no-op when target holds a slot in another available collection`() = runBlocking<Unit> {
        withAvailableCollections(collection, liteCollection)
        withNoStaleAllocations()
        withChainSlots(
            StatementSlotsForCollection(collection, listOf(StatementStoreSlot.Free(seq = 0u))),
            StatementSlotsForCollection(liteCollection, takenSlots(target)),
        )

        val result = allocate(OnExistingAllocationStrategy.IGNORE, SlotPriority.Normal)

        assertSuccess(result)
        verifyNoAllocationInsertions()
        verifyNoExtrinsicSubmitted()
    }

    @Test
    fun `eviction pool spans collections - High caller evicts Normal slot in LitePeople`() = runBlocking<Unit> {
        withAvailableCollections(collection, liteCollection)
        withNoStaleAllocations()
        withZeroCooldown()
        withChainSlots(
            StatementSlotsForCollection(collection, takenSlots(accountHigh)),
            StatementSlotsForCollection(liteCollection, takenSlots(accountNormal)),
        )
        withTrackedPriorities(
            accountHigh to SlotPriority.High,
            accountNormal to SlotPriority.Normal,
        )
        withSuccessfulSubmission()

        val result = allocate(OnExistingAllocationStrategy.INCREASE, SlotPriority.High)

        assertSuccess(result)
        verifySlotDeletedFor(accountNormal, seq = 0u, inCollection = liteCollection)
        verifyNoEvictionOf(accountHigh)
        verifyAllocationInsertedFor(target, seq = 0u, priority = SlotPriority.High, inCollection = liteCollection)
    }

    // -- setup helpers (every used method gets an explicit stub) --

    private suspend fun withChainContext() {
        whenever(chain.id).thenReturn(chainId)
        withAvailableCollections(collection)
    }

    private suspend fun withAvailableCollections(vararg collections: PeopleCollection) {
        whenever(contextResolver.resolve())
            .thenReturn(Result.success(AllocateContext(chain, collections.toList(), period)))
    }

    private suspend fun withRenewerSucceeds() {
        whenever(renewer.renew(any(), any())).thenReturn(Result.success(Unit))
    }

    private suspend fun withNoStaleAllocations() {
        whenever(allocationRepository.hasStaleFor(any(), any(), anyUInt())).thenReturn(false)
    }

    private suspend fun withStaleAllocationsFor(account: AccountId) {
        whenever(allocationRepository.hasStaleFor(any(), eq(account), anyUInt())).thenReturn(true)
    }

    /** The chain has a single slot at seq 0, taken by [account]; no free seqs. */
    private suspend fun withSlotTakenBy(account: AccountId) {
        withSlotsTakenBy(account)
    }

    /**
     * The chain has one slot per supplied account, oldest-to-newest. seq increments by
     * position; on-chain `since` increments by 100s per position so LRU within the chain
     * matches the argument order.
     */
    private suspend fun withSlotsTakenBy(vararg accountsOldestToNewest: AccountId) {
        withChainSlots(StatementStoreSlots(listOf(StatementSlotsForCollection(collection, takenSlots(*accountsOldestToNewest)))))
    }

    /** The chain has a single free slot at seq 0; nothing taken. */
    private suspend fun withFreeSlot() {
        withChainSlots(StatementStoreSlots(listOf(StatementSlotsForCollection(collection, listOf(StatementStoreSlot.Free(seq = 0u))))))
    }

    private fun takenSlots(vararg accountsOldestToNewest: AccountId): List<StatementStoreSlot> {
        return accountsOldestToNewest.mapIndexed { idx, account ->
            StatementStoreSlot.Taken(
                seq = idx.toUInt(),
                accountId = account,
                since = fixedNow - (1000 - idx * 100).seconds,
            )
        }
    }

    /** Cooldown is zero, so any taken slot is immediately replaceable. */
    private suspend fun withZeroCooldown() {
        whenever(slotRepository.replacementCooldown()).thenReturn(Result.success(0.seconds))
    }

    private suspend fun withChainSlots(slots: StatementStoreSlots) {
        whenever(slotLoader.loadSlots(any())).thenReturn(Result.success(slots))
    }

    private suspend fun withChainSlots(vararg perCollection: StatementSlotsForCollection) {
        withChainSlots(StatementStoreSlots(perCollection.toList()))
    }

    /** Default: the bulk priority lookup returns an empty map (no accounts tracked). */
    private suspend fun withNoTrackedPriorities() {
        whenever(allocationRepository.maxPriorityLevelsFor(any(), any())).thenReturn(emptyMap())
    }

    private suspend fun withTrackedPriorities(vararg entries: Pair<AccountId, SlotPriority>) {
        whenever(allocationRepository.maxPriorityLevelsFor(any(), any())).thenReturn(entries.toMap())
    }

    private suspend fun withSuccessfulSubmission() {
        whenever(origins.asResourcesStatementStoreSlot(anyUInt(), anyUInt(), any()))
            .thenReturn(Result.success(mock(TransactionOrigin::class.java)))
        whenever(
            extrinsicService.submitExtrinsicAndAwaitExecution(any(), any(), any(), any(), any())
        ).thenReturn(Result.success(successExecution()))
    }

    // -- assertion / verification helpers --

    private fun assertSuccess(result: Result<*>) {
        assertTrue("expected Result.success but was ${result.exceptionOrNull()}", result.isSuccess)
    }

    private fun assertNoAllocationAvailable(result: Result<Unit>) {
        assertTrue(result.isFailure)
        assertTrue(
            "expected NoAllocationAvailable but was ${result.exceptionOrNull()}",
            result.exceptionOrNull() is StatementStoreSlotAllocationError.NoAllocationAvailable,
        )
    }

    private suspend fun assertRenewerInvokedWith(priorityAccount: AccountId) {
        verify(renewer, times(1)).renew(any(), eq(priorityAccount))
    }

    private suspend fun assertRenewerNotInvoked() {
        verify(renewer, never()).renew(any(), any())
    }

    private suspend fun verifyNoAllocationInsertions() {
        verify(allocationRepository, never()).insert(any(), any(), any(), anyUInt(), anyUInt(), any(), any())
    }

    private suspend fun verifyNoEvictionPerformed() {
        verify(allocationRepository, never()).deleteSlot(any(), any(), any(), anyUInt())
    }

    private suspend fun verifySlotDeletedFor(account: AccountId, seq: UInt, inCollection: PeopleCollection = collection) {
        verify(allocationRepository, times(1)).deleteSlot(eq(chainId), eq(inCollection), eq(account), eqUInt(seq))
    }

    private suspend fun verifyNoEvictionOf(account: AccountId) {
        verify(allocationRepository, never()).deleteSlot(any(), any(), eq(account), anyUInt())
    }

    private suspend fun verifyAllocationInsertedFor(
        account: AccountId,
        seq: UInt,
        priority: SlotPriority,
        inCollection: PeopleCollection = collection,
    ) {
        verify(allocationRepository, times(1)).insert(
            chainId = eq(chainId),
            collection = eq(inCollection),
            accountId = eq(account),
            seq = eqUInt(seq),
            latestRenewedPeriod = anyUInt(),
            since = any(),
            priority = eq(priority),
        )
    }

    private suspend fun verifyNoExtrinsicSubmitted() {
        verify(extrinsicService, never()).submitExtrinsicAndAwaitExecution(any(), any(), any(), any(), any())
    }

    // -- value/object helpers --

    private fun successExecution(): ExtrinsicExecutionResult = ExtrinsicExecutionResult(
        extrinsicHash = "0xext",
        blockHash = "0xblk",
        outcome = ExtrinsicDispatch.Ok(emittedEvents = emptyList()),
    )

    private suspend fun allocate(strategy: OnExistingAllocationStrategy, priority: SlotPriority): Result<Unit> {
        return with(StalenessReportCollector.NoOp) { allocator.allocate(target, strategy, priority) }
    }
}
