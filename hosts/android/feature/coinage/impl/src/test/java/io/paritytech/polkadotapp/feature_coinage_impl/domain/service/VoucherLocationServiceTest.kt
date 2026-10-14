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
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
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

        assertEquals(RecyclerFungibility.ofPercent(100), written.single().values.single().recyclerFungibility)
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

    private fun givenIncludedPosition() {
        val position = RingPosition.Included(
            ringIndex = RecyclerIndex(RING_INDEX.toBigInteger()),
            ringPage = 0,
            ringPosition = 0
        )
        every { membersRepository.subscribeMembers(any(), any(), any()) } returns
            flowOf(Result.success(mapOf((collectionId() to VOUCHER_KEY) to position)))
    }

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

    private fun recyclerKey() = RecyclerKey(DENOMINATION, RecyclerIndex(RING_INDEX.toBigInteger()))

    private fun collectionId(): RingCollectionId = DENOMINATION.toRingCollectionId(INSTANCE_ID)

    private fun voucher() = RecyclerVoucher(
        ringVrfKeyIndex = testKey(0),
        ringVrfPublicKey = VOUCHER_KEY,
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
        const val RING_KEYS_PER_PAGE = 255
        val INSTANCE_ID: CoinageInstanceId = 1u
        val NOW: Instant = Instant.fromEpochMilliseconds(1_000L)
        val DENOMINATION = ValueExponent(3)
        val VOUCHER_KEY: BandersnatchPublicKey = ByteArray(32) { 7 }.toDataByteArray()
    }
}
