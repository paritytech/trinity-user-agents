package io.paritytech.polkadotapp.feature_statement_store_impl.domain.notificationAllocator

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationAllocationError
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator.AllocateContext
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator.AllocateContextResolver
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator.CurrentPeriodProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTransactionService
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxState
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import kotlin.time.Duration.Companion.seconds

class RealNotificationStatementAccountAllocatorTest {
    private val period = 20_000u
    private val first: AccountId = byteArrayOf(0x01).toDataByteArray()
    private val second: AccountId = byteArrayOf(0x02).toDataByteArray()
    private val third: AccountId = byteArrayOf(0x03).toDataByteArray()
    private val chain: Chain = mockk()
    private val context = AllocateContext(chain, listOf(PeopleCollection.People), period)

    private val durableTransactionService: DurableTransactionService = mockk()
    private val contextResolver: AllocateContextResolver = mockk()
    private val seqPicker: NotificationSeqPicker = mockk()
    private val reservations = NotificationSeqReservations()

    private val allocator = RealNotificationStatementAccountAllocator(
        durableTransactionService, contextResolver, seqPicker, reservations, NotificationSlotAllocationLock(),
        CurrentPeriodProvider { period },
    )

    @Before
    fun setUp() {
        coEvery { contextResolver.resolve() } returns Result.success(context)
        coEvery { durableTransactionService.schedule(any(), any(), any(), any()) } returns Result.success(listOf(DurableTxId(1)))
    }

    @Test
    fun `initiateAllocation schedules a claim when a slot is free`() = runBlocking<Unit> {
        withNoClaims(first)
        withFreeSlotCount(1)

        assertTrue(allocator.initiateAllocation(first).isSuccess)

        verifyClaimScheduled(first)
    }

    @Test
    fun `initiateAllocation does not schedule again while a claim is live`() = runBlocking<Unit> {
        withClaim(first, DurableTxStatus.PENDING_SUBMISSION)
        withFreeSlotCount(1)

        assertTrue(allocator.initiateAllocation(first).isSuccess)

        verifyNoClaimScheduled(first)
    }

    @Test
    fun `initiateAllocation retries a target whose earlier claim gave up`() = runBlocking<Unit> {
        withClaim(first, DurableTxStatus.FAILURE)
        withFreeSlotCount(1)

        assertTrue(allocator.initiateAllocation(first).isSuccess)

        verifyClaimScheduled(first)
    }

    @Test
    fun `initiateAllocation fails with NoFreeSlotInPeriod when no slot is free`() = runBlocking<Unit> {
        withNoClaims(first)
        withFreeSlotCount(0)

        val error = allocator.initiateAllocation(first).exceptionOrNull()

        assertTrue(error is NotificationAllocationError.NoFreeSlotInPeriod)
    }

    @Test
    fun `initiateAllocations schedules leading targets up to the free slot count`() = runBlocking<Unit> {
        withNoClaims(first, second, third)
        withFreeSlotCount(2)

        val allocated = allocator.initiateAllocations(listOf(first, second, third)).getOrThrow()

        assertEquals(listOf(first, second), allocated)
        verifyNoClaimScheduled(third)
    }

    @Test
    fun `reserves the slot at scheduling so a following allocation cannot take it`() = runBlocking<Unit> {
        withNoClaims(first, second)
        withFreeSlotCount(1)

        assertTrue(allocator.initiateAllocation(first).isSuccess)
        val error = allocator.initiateAllocation(second).exceptionOrNull()

        assertTrue(error is NotificationAllocationError.NoFreeSlotInPeriod)
        assertEquals(NotificationSlot(PeopleCollection.People, period, 0u), reservations.reservedFor(first))
    }

    @Test
    fun `awaitAllocated succeeds once a claim has executed`() = runBlocking<Unit> {
        withGroupStatesOver(first, DurableTxStatus.FAILURE, DurableTxStatus.PENDING_SUCCESS)

        assertTrue(allocator.awaitAllocated(first, 1.seconds).isSuccess)
    }

    @Test
    fun `awaitAllocated fails with NoFreeSlotInPeriod when every claim gave up`() = runBlocking<Unit> {
        withGroupStatesOver(first, DurableTxStatus.FAILURE)

        val error = allocator.awaitAllocated(first, 1.seconds).exceptionOrNull()

        assertTrue(error is NotificationAllocationError.NoFreeSlotInPeriod)
    }

    private fun withNoClaims(vararg targets: AccountId) {
        targets.forEach { target ->
            coEvery { durableTransactionService.getGroupStates(NOTIFICATION_SLOT_DOMAIN, target.notificationSlotGroup()) } returns
                Result.success(emptyList())
        }
    }

    private fun withClaim(target: AccountId, status: DurableTxStatus) {
        coEvery { durableTransactionService.getGroupStates(NOTIFICATION_SLOT_DOMAIN, target.notificationSlotGroup()) } returns
            Result.success(listOf(DurableTxState(DurableTxId(1), status)))
    }

    private fun withGroupStatesOver(target: AccountId, vararg statuses: DurableTxStatus) {
        val states = statuses.mapIndexed { index, status -> DurableTxState(DurableTxId(index.toLong()), status) }
        every { durableTransactionService.subscribeGroupStates(NOTIFICATION_SLOT_DOMAIN, target.notificationSlotGroup()) } returns
            flowOf(states)
    }

    // Mirrors the real picker: slots reserved for any account stop being free.
    private fun withFreeSlotCount(count: Int) {
        val slots = (0 until count).map { NotificationSlot(PeopleCollection.People, period, it.toUByte()) }
        coEvery { seqPicker.freeSlots(context, forTarget = null) } answers {
            Result.success(slots - reservations.reservedIn(period, exceptFor = null))
        }
    }

    private fun verifyClaimScheduled(target: AccountId) {
        coVerify(exactly = 1) {
            durableTransactionService.schedule(NOTIFICATION_SLOT_DOMAIN, target.notificationSlotGroup(), listOf(target.notificationSlotPolicy()), any())
        }
    }

    private fun verifyNoClaimScheduled(target: AccountId) {
        coVerify(exactly = 0) { durableTransactionService.schedule(NOTIFICATION_SLOT_DOMAIN, target.notificationSlotGroup(), any(), any()) }
    }
}
