package io.paritytech.polkadotapp.feature_coinage_impl.domain.common

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.ExponentBoundsRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.FakeVoucherRingDerivation
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.test_shared.assertFailure
import io.paritytech.polkadotapp.test_shared.settle
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class RealVoucherAllocatorTest {
    private val voucherRepository = mockk<VoucherRepository>()
    private val gate = HeldInstallationGate()
    private val saved = mutableListOf<RecyclerVoucher>()

    private val allocator = RealVoucherAllocator(
        voucherRepository = voucherRepository,
        currentInstallationProvider = gate,
        voucherRingDerivation = FakeVoucherRingDerivation(),
        boundsRepository = mockk<ExponentBoundsRepository> {
            coEvery { minExponent(CHAIN) } returns Result.success(0)
            coEvery { maxExponent(CHAIN) } returns Result.success(30)
        },
        chainAssetProvider = mockk<ChainAssetProvider> { every { chainId() } returns CHAIN },
    )

    @Before
    fun setUp() {
        coEvery { voucherRepository.getNextDerivationIndex(TEST_INSTALLATION) } answers { NEXT_INDEX + saved.size }
        coEvery { voucherRepository.saveNew(any<RecyclerVoucher>()) } answers { saved += firstArg<RecyclerVoucher>() }
        coEvery { voucherRepository.saveNew(any<List<RecyclerVoucher>>()) } answers { saved += firstArg<List<RecyclerVoucher>>() }
    }

    @Test
    fun `allocate waits for the gate and uses its installation`() = runBlocking<Unit> {
        val call = async(start = CoroutineStart.UNDISPATCHED) { allocator.allocate(ValueExponent(3)) }
        settle()

        assertFalse(call.isCompleted)
        assertEquals(emptyList<RecyclerVoucher>(), saved)

        gate.release(TEST_INSTALLATION)

        val voucher = call.await().getOrThrow()
        assertEquals(CoinageKeyIndex(TEST_INSTALLATION, NEXT_INDEX), voucher.ringVrfKeyIndex)
        assertEquals(listOf(voucher), saved)
    }

    @Test
    fun `allocateAll waits for the gate and uses its installation`() = runBlocking<Unit> {
        val call = async(start = CoroutineStart.UNDISPATCHED) { allocator.allocateAll(listOf(ValueExponent(1), ValueExponent(2))) }
        settle()

        assertFalse(call.isCompleted)
        assertEquals(emptyList<RecyclerVoucher>(), saved)

        gate.release(TEST_INSTALLATION)

        val indices = call.await().getOrThrow().map { it.ringVrfKeyIndex }
        assertEquals(listOf(CoinageKeyIndex(TEST_INSTALLATION, NEXT_INDEX), CoinageKeyIndex(TEST_INSTALLATION, NEXT_INDEX + 1)), indices)
    }

    @Test
    fun `a held gate does not hold the allocation mutex`() = runBlocking<Unit> {
        val first = async(start = CoroutineStart.UNDISPATCHED) { allocator.allocate(ValueExponent(3)) }
        val second = async(start = CoroutineStart.UNDISPATCHED) { allocator.allocate(ValueExponent(3)) }
        settle()
        assertEquals(2, gate.waitingCallers)

        gate.releaseLatestCaller(TEST_INSTALLATION)
        settle()

        assertTrue(second.isCompleted)
        assertFalse(first.isCompleted)

        gate.release(TEST_INSTALLATION)
        val indices = listOf(first.await(), second.await()).map { it.getOrThrow().ringVrfKeyIndex.item }.sorted()
        assertEquals(listOf(NEXT_INDEX, NEXT_INDEX + 1), indices)
    }

    @Test
    fun `an invalid exponent fails without waiting for the gate`() = runBlocking<Unit> {
        assertFailure(allocator.allocate(ValueExponent(31)))
        assertFailure(allocator.allocateAll(listOf(ValueExponent(1), ValueExponent(31))))
        assertEquals(0, gate.waitingCallers)
        assertEquals(emptyList<RecyclerVoucher>(), saved)
    }

    @Test
    fun `allocateAll with no exponents waits on the gate and returns an empty list`() = runBlocking<Unit> {
        val call = async(start = CoroutineStart.UNDISPATCHED) { allocator.allocateAll(emptyList()) }
        settle()
        assertFalse(call.isCompleted)

        gate.release(TEST_INSTALLATION)

        assertEquals(Result.success(emptyList<RecyclerVoucher>()), call.await())
    }

    @Test
    fun `a failed installation read gives a failed result`() = runBlocking<Unit> {
        gate.fail(IllegalStateException("installation table unreadable"))

        assertFailure(allocator.allocate(ValueExponent(3)))
        assertFailure(allocator.allocateAll(listOf(ValueExponent(1))))
        assertEquals(emptyList<RecyclerVoucher>(), saved)
    }

    @Test
    fun `a cancelled wait is rethrown, not turned into a failed result`() = runBlocking<Unit> {
        var result: Result<RecyclerVoucher>? = null
        val call = launch(start = CoroutineStart.UNDISPATCHED) { result = allocator.allocate(ValueExponent(3)) }
        settle()

        call.cancel()
        settle()

        assertEquals(null, result)
    }
    private companion object {
        const val CHAIN = "people"
        const val NEXT_INDEX = 41
    }
}
