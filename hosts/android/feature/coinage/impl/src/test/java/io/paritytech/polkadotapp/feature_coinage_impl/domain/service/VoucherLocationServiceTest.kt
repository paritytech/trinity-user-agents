package io.paritytech.polkadotapp.feature_coinage_impl.domain.service

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstanceId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.toRingCollectionId
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinageInstanceIdProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRecyclerUpdate
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.RingCapacityProvider
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import io.paritytech.polkadotapp.feature_members_api.data.model.RingStatus
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersRepository
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Instant

/**
 * Keeping each voucher's ring position and fungibility current.
 *
 * The delicate part is that a fungibility is only as trustworthy as the three chain reads behind it. A read
 * that failed says nothing, and writing a number anyway would overstate the privacy the user has — and could
 * freeze that overstatement permanently, since the maximum is written once and never again.
 */
class VoucherLocationServiceTest {
    private val chainAssetProvider: ChainAssetProvider = mockk()
    private val voucherRepository: VoucherRepository = mockk(relaxUnitFun = true)
    private val membersRepository: MembersRepository = mockk()
    private val instanceIdProvider: CoinageInstanceIdProvider = mockk()
    private val ringCapacityProvider: RingCapacityProvider = mockk()
    private val timeProvider: TimeProvider = mockk()

    private val service = VoucherLocationService(
        chainAssetProvider,
        voucherRepository,
        membersRepository,
        instanceIdProvider,
        ringCapacityProvider,
        timeProvider
    )

    private val written = mutableListOf<Map<BandersnatchPublicKey, VoucherRecyclerUpdate>>()
    private val scope = CoroutineScope(UnconfinedTestDispatcher())

    @After
    fun stop() = scope.cancel()

    @Test
    fun `a readable ring has its fungibility written`() = runTest {
        givenChain()
        givenVoucherIncludedInRing()
        givenRingStatus(included = RING_CAPACITY)
        givenCapacityReadable()
        givenUnloadedCounts(Result.success(mapOf(recyclerKey() to 0)))

        startService()

        val update = written.single().values.single()
        assertEquals(RecyclerFungibility.ofPercent(100), update.recyclerFungibility)
        assertEquals(RecyclerFungibility.ofPercent(100), update.maxRecyclerFungibility)
    }

    /** Within a successful response an absent recycler really has had nothing unloaded. */
    @Test
    fun `a recycler the runtime holds no entry for counts as nothing unloaded`() = runTest {
        givenChain()
        givenVoucherIncludedInRing()
        givenRingStatus(included = RING_CAPACITY)
        givenCapacityReadable()
        givenUnloadedCounts(Result.success(emptyMap()))

        startService()

        val update = written.single().values.single()
        assertEquals(RecyclerFungibility.ofPercent(100), update.recyclerFungibility)
        assertEquals(0, update.location.recyclerUnloaded)
    }

    /**
     * The regression this class exists for: a failed read must not be read as "nothing unloaded".
     *
     * Doing so reports every ring as untouched, which inflates the fungibility on screen, and if the failure
     * lands on a voucher's first tick in a ring it freezes that inflated figure for good.
     */
    @Test
    fun `a failed unloaded count read writes no fungibility at all`() = runTest {
        givenChain()
        givenVoucherIncludedInRing()
        givenRingStatus(included = RING_CAPACITY)
        givenCapacityReadable()
        givenUnloadedCounts(Result.failure(IllegalStateException("subscription dropped")))

        startService()

        val update = written.single().values.single()
        assertNull("fungibility must be left standing, not guessed", update.recyclerFungibility)
        assertNull("a guessed maximum would freeze permanently", update.maxRecyclerFungibility)
    }

    /** The location is what decides spendability, so it must survive a failed fungibility read. */
    @Test
    fun `a failed unloaded count read still records where the voucher sits`() = runTest {
        givenChain()
        givenVoucherIncludedInRing()
        givenRingStatus(included = RING_MEMBERS)
        givenCapacityReadable()
        givenUnloadedCounts(Result.failure(IllegalStateException("subscription dropped")))

        startService()

        val update = written.single().values.single()
        assertEquals(RING_MEMBERS, update.location.recyclerMembers)
    }

    @Test
    fun `a failed capacity read also writes no fungibility`() = runTest {
        givenChain()
        givenVoucherIncludedInRing()
        givenRingStatus(included = RING_CAPACITY)
        coEvery { ringCapacityProvider.capacitiesFor(any()) } returns
            Result.failure(IllegalStateException("no metadata"))
        givenUnloadedCounts(Result.success(mapOf(recyclerKey() to 0)))

        startService()

        assertTrue(written.single().values.all { it.recyclerFungibility == null })
    }

    /**
     * The clock is read per tick, not per voucher: the DAO keeps the earlier instant while a voucher stays
     * in the same ring, and takes this one when it moves — so what the service must supply is simply *now*.
     */
    @Test
    fun `the written location carries the moment the ring was read`() = runTest {
        givenChain()
        givenVoucherIncludedInRing()
        givenRingStatus(included = RING_MEMBERS)
        givenCapacityReadable()
        givenUnloadedCounts(Result.success(mapOf(recyclerKey() to 0)))

        startService()

        assertEquals(NOW, written.single().values.single().location.enteredAt)
    }

    /** A voucher whose stored entry time was cleared is re-read, so a restore starts its wait again. */
    @Test
    fun `a voucher that lost its entry time is written again`() = runTest {
        givenChain()
        val vouchers = MutableStateFlow(listOf(voucher()))
        every { voucherRepository.subscribeAllVouchers() } returns vouchers
        givenIncludedPosition()
        givenRingStatus(included = RING_MEMBERS)
        givenCapacityReadable()
        givenUnloadedCounts(Result.success(mapOf(recyclerKey() to 0)))

        startService()
        val located = written.single().values.single().location
        vouchers.value = listOf(voucher().copy(location = located))
        val beforeRestore = written.size
        vouchers.value = listOf(voucher().copy(location = located.copy(enteredAt = null)))

        assertEquals("clearing the entry time must re-read the ring", beforeRestore + 1, written.size)
        assertEquals(NOW, written.last().values.single().location.enteredAt)
    }

    @Test
    fun `the ring's unloaded count reaches the voucher`() = runTest {
        givenChain()
        givenTwoVouchersInDifferentRings()
        givenCapacityReadable()
        givenUnloadedCounts(Result.success(mapOf(recyclerKey(RING_INDEX) to 5, recyclerKey(OTHER_RING_INDEX) to 2)))

        startService()

        val updates = written.single()
        assertEquals(5, updates.getValue(VOUCHER_KEY).location.recyclerUnloaded)
        assertEquals(2, updates.getValue(OTHER_VOUCHER_KEY).location.recyclerUnloaded)
    }

    @Test
    fun `a failed unloaded count read writes no unloaded count`() = runTest {
        givenChain()
        givenVoucherIncludedInRing()
        givenRingStatus(included = RING_MEMBERS)
        givenCapacityReadable()
        givenUnloadedCounts(Result.failure(IllegalStateException("subscription dropped")))

        startService()

        assertNull(written.single().values.single().location.recyclerUnloaded)
    }

    @Test
    fun `a failed unloaded count read is retried until it succeeds`() = runTest {
        givenChain()
        givenVoucherIncludedInRing()
        givenRingStatus(included = RING_MEMBERS)
        givenCapacityReadable()
        var attempt = 0
        every { voucherRepository.subscribeUnloadedCounts(any(), any(), any()) } returns flow {
            attempt++
            if (attempt == 1) {
                emit(Result.failure(IllegalStateException("subscription dropped")))
            } else {
                emit(Result.success(mapOf(recyclerKey() to 3)))
            }
        }

        with(ComputationalScope(backgroundScope)) { service.start() }
        runCurrent()
        assertNull(written.single().values.single().location.recyclerUnloaded)
        advanceTimeBy(READ_RESUBSCRIBE_DELAY.delayFor(attempt = 1))
        runCurrent()
        assertEquals(3, written.last().values.single().location.recyclerUnloaded)

        advanceTimeBy(1.minutes)
        runCurrent()

        assertEquals("a read that succeeded must not be resubscribed", 2, attempt)
    }

    @Test
    fun `a read that keeps failing waits longer each time`() = runTest {
        givenChain()
        givenVoucherIncludedInRing()
        givenRingStatus(included = RING_MEMBERS)
        givenCapacityReadable()
        var attempt = 0
        every { voucherRepository.subscribeUnloadedCounts(any(), any(), any()) } returns flow {
            attempt++
            emit(Result.failure(IllegalStateException("subscription dropped")))
        }

        with(ComputationalScope(backgroundScope)) { service.start() }
        runCurrent()
        advanceTimeBy(READ_RESUBSCRIBE_DELAY.delayFor(attempt = 1))
        runCurrent()
        assertEquals(2, attempt)

        advanceTimeBy(READ_RESUBSCRIBE_DELAY.delayFor(attempt = 2))
        runCurrent()

        assertEquals("a flat delay would have reached a fourth attempt by now", 3, attempt)
    }

    /** A drop after a healthy stretch waits the first delay again, not the one the early failures reached. */
    @Test
    fun `a subscription that drops after good reads starts its wait over`() = runTest {
        givenChain()
        givenVoucherIncludedInRing()
        givenRingStatus(included = RING_MEMBERS)
        givenCapacityReadable()
        var attempt = 0
        every { voucherRepository.subscribeUnloadedCounts(any(), any(), any()) } returns flow {
            attempt++
            if (attempt == 2) emit(Result.success(mapOf(recyclerKey() to 3)))
            emit(Result.failure(IllegalStateException("subscription dropped")))
        }

        with(ComputationalScope(backgroundScope)) { service.start() }
        runCurrent()
        advanceTimeBy(READ_RESUBSCRIBE_DELAY.delayFor(attempt = 1))
        runCurrent()
        assertTrue(written.any { it.values.single().location.recyclerUnloaded == 3 })

        advanceTimeBy(READ_RESUBSCRIBE_DELAY.delayFor(attempt = 1))
        runCurrent()

        assertEquals("the good read must reset the wait", 3, attempt)
    }

    /**
     * An empty status map is a legitimate answer — no ring holds these vouchers — so a failed read that
     * reported one would drop every voucher from the write, and no later good read could undo it.
     */
    @Test
    fun `a failed ring status read is retried and the recovered status reaches the write`() = runTest {
        givenChain()
        givenVoucherIncludedInRing()
        givenCapacityReadable()
        givenUnloadedCounts(Result.success(mapOf(recyclerKey() to 0)))
        var attempt = 0
        every { membersRepository.subscribeRingStatuses(any(), any()) } returns flow {
            attempt++
            if (attempt == 1) {
                emit(Result.failure(IllegalStateException("subscription dropped")))
            } else {
                emit(Result.success(mapOf((collectionId() to RecyclerIndex(RING_INDEX.toBigInteger())) to RingStatus(RING_MEMBERS, RING_MEMBERS))))
            }
        }

        with(ComputationalScope(backgroundScope)) { service.start() }
        runCurrent()
        assertTrue("a failed read must write no voucher at all", written.all { it.isEmpty() })

        advanceTimeBy(READ_RESUBSCRIBE_DELAY.delayFor(attempt = 1))
        runCurrent()

        assertEquals(RING_MEMBERS, written.last().values.single().location.recyclerMembers)
    }

    private suspend fun startService() {
        with(ComputationalScope(scope)) { service.start() }
    }

    private fun givenChain() {
        val asset: Chain.Asset = mockk()
        every { asset.chainId } returns CHAIN_ID
        every { chainAssetProvider.chainId() } returns CHAIN_ID
        coEvery { instanceIdProvider.instanceId() } returns Result.success(INSTANCE_ID)
        coEvery { membersRepository.getRingKeysPageSize(any()) } returns Result.success(RING_KEYS_PER_PAGE)
        coEvery { voucherRepository.updateRecyclerState(any()) } answers {
            written += firstArg<Map<BandersnatchPublicKey, VoucherRecyclerUpdate>>()
        }
        every { timeProvider.now() } returns NOW
    }

    private fun givenVoucherIncludedInRing() {
        every { voucherRepository.subscribeAllVouchers() } returns flowOf(listOf(voucher()))

        givenIncludedPosition()
    }

    private fun givenTwoVouchersInDifferentRings() {
        every { voucherRepository.subscribeAllVouchers() } returns
            flowOf(listOf(voucher(), voucher(index = 1, key = OTHER_VOUCHER_KEY)))
        every { membersRepository.subscribeMembers(any(), any(), any()) } returns flowOf(
            Result.success(
                mapOf(
                    (collectionId() to VOUCHER_KEY) to includedIn(RING_INDEX),
                    (collectionId() to OTHER_VOUCHER_KEY) to includedIn(OTHER_RING_INDEX),
                )
            )
        )
        every { membersRepository.subscribeRingStatuses(any(), any()) } returns flowOf(
            Result.success(
                listOf(RING_INDEX, OTHER_RING_INDEX).associate { ring ->
                    (collectionId() to RecyclerIndex(ring.toBigInteger())) to
                        RingStatus(total = RING_MEMBERS, included = RING_MEMBERS)
                }
            )
        )
    }

    private fun givenIncludedPosition() {
        every { membersRepository.subscribeMembers(any(), any(), any()) } returns
            flowOf(Result.success(mapOf((collectionId() to VOUCHER_KEY) to includedIn(RING_INDEX))))
    }

    private fun includedIn(ring: Int) = RingPosition.Included(
        ringIndex = RecyclerIndex(ring.toBigInteger()),
        ringPage = 0,
        ringPosition = 0
    )

    private fun givenRingStatus(included: Int) {
        every { membersRepository.subscribeRingStatuses(any(), any()) } returns
            flowOf(
                Result.success(
                    mapOf(
                        (collectionId() to RecyclerIndex(RING_INDEX.toBigInteger())) to
                            RingStatus(total = included, included = included)
                    )
                )
            )
    }

    private fun givenCapacityReadable() {
        coEvery { ringCapacityProvider.capacitiesFor(any()) } returns
            Result.success(mapOf(DENOMINATION to RING_CAPACITY))
    }

    private fun givenUnloadedCounts(result: Result<Map<RecyclerKey, Int>>) {
        every { voucherRepository.subscribeUnloadedCounts(any(), any(), any()) } returns flowOf(result)
    }

    private fun recyclerKey(ring: Int = RING_INDEX) = RecyclerKey(DENOMINATION, RecyclerIndex(ring.toBigInteger()))

    private fun collectionId(): RingCollectionId = DENOMINATION.toRingCollectionId(INSTANCE_ID)

    private fun voucher(index: Int = 0, key: BandersnatchPublicKey = VOUCHER_KEY) = RecyclerVoucher(
        ringVrfKeyIndex = testKey(index),
        ringVrfPublicKey = key,
        recyclerValue = DENOMINATION,
        location = RecyclerVoucher.Location.Unknown,
        recyclerFungibility = RecyclerFungibility.NONE,
        maxRecyclerFungibility = null
    )

    private companion object {
        const val CHAIN_ID = "test-chain"
        const val RING_CAPACITY = 767
        const val RING_MEMBERS = 400
        const val RING_INDEX = 3
        const val OTHER_RING_INDEX = 4
        const val RING_KEYS_PER_PAGE = 255
        val INSTANCE_ID: CoinageInstanceId = 1u
        val NOW: Instant = Instant.fromEpochMilliseconds(1_000L)
        val DENOMINATION = ValueExponent(3)
        val VOUCHER_KEY: BandersnatchPublicKey = ByteArray(32) { 7 }.toDataByteArray()
        val OTHER_VOUCHER_KEY: BandersnatchPublicKey = ByteArray(32) { 8 }.toDataByteArray()
    }
}
