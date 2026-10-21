package io.paritytech.polkadotapp.feature_coinage_impl.domain.common

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.CoinKeypairDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.ExponentBoundsRepository
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.test_shared.assertFailure
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class RealCoinAllocatorTest {
    private val coinRepository = mockk<CoinRepository>(relaxed = true)
    private val gate = HeldInstallationGate()
    private var nextIndex = NEXT_INDEX

    private val allocator = RealCoinAllocator(
        coinRepository = coinRepository,
        currentInstallationProvider = gate,
        keypairDerivation = mockk<CoinKeypairDerivation> {
            coEvery { deriveKeypair(any()) } returns mockk { every { publicKey } returns ByteArray(32) }
        },
        boundsRepository = mockk<ExponentBoundsRepository> {
            coEvery { minExponent(CHAIN) } returns Result.success(0)
            coEvery { maxExponent(CHAIN) } returns Result.success(30)
        },
        chainAssetProvider = mockk<ChainAssetProvider> { every { chainId() } returns CHAIN },
    )

    @Before
    fun setUp() {
        coEvery { coinRepository.getNextDerivationIndex(TEST_INSTALLATION) } answers { nextIndex }
        coEvery { coinRepository.saveNew(any<Coin>()) } answers { nextIndex++ }
        coEvery { coinRepository.saveNew(any<List<Coin>>()) } answers { nextIndex += firstArg<List<Coin>>().size }
    }

    @Test
    fun `an allocated coin takes the next index of the current installation`() = runTest {
        gate.release(TEST_INSTALLATION)

        val coin = allocator.allocate(ValueExponent(3), CoinProvenance.UNKNOWN).getOrThrow()

        assertEquals(CoinageKeyIndex(TEST_INSTALLATION, NEXT_INDEX), coin.derivationIndex)
        coVerify { coinRepository.saveNew(coin) }
    }

    @Test
    fun `an allocation whose index is already taken fails instead of overwriting the row`() = runTest {
        gate.release(TEST_INSTALLATION)
        coEvery { coinRepository.saveNew(any<Coin>()) } throws IllegalStateException("UNIQUE constraint failed: coins")

        val result = allocator.allocate(ValueExponent(3), CoinProvenance.UNKNOWN)

        assertFailure(result)
        coVerify(exactly = 0) { coinRepository.save(any()) }
    }

    @Test
    fun `a batch whose indices are already taken fails instead of overwriting the rows`() = runTest {
        gate.release(TEST_INSTALLATION)
        coEvery { coinRepository.saveNew(any<List<Coin>>()) } throws IllegalStateException("UNIQUE constraint failed: coins")

        val result = allocator.allocateAll(listOf(ValueExponent(1), ValueExponent(2)), CoinProvenance.UNKNOWN)

        assertFailure(result)
        coVerify(exactly = 0) { coinRepository.saveAll(any()) }
    }

    @Test
    fun `allocate waits for the gate and uses its installation`() = runTest {
        val call = async(start = CoroutineStart.UNDISPATCHED) { allocator.allocate(ValueExponent(3), CoinProvenance.UNKNOWN) }
        runCurrent()

        assertFalse(call.isCompleted)
        coVerify(exactly = 0) { coinRepository.saveNew(any<Coin>()) }

        gate.release(TEST_INSTALLATION)

        val coin = call.await().getOrThrow()
        assertEquals(CoinageKeyIndex(TEST_INSTALLATION, NEXT_INDEX), coin.derivationIndex)
        coVerify(exactly = 1) { coinRepository.saveNew(coin) }
    }

    @Test
    fun `allocateAll waits for the gate and uses its installation`() = runTest {
        val call = async(start = CoroutineStart.UNDISPATCHED) {
            allocator.allocateAll(listOf(ValueExponent(1), ValueExponent(2)), CoinProvenance.UNKNOWN)
        }
        runCurrent()

        assertFalse(call.isCompleted)
        coVerify(exactly = 0) { coinRepository.saveNew(any<List<Coin>>()) }

        gate.release(TEST_INSTALLATION)

        val indices = call.await().getOrThrow().map { it.derivationIndex }
        assertEquals(listOf(CoinageKeyIndex(TEST_INSTALLATION, NEXT_INDEX), CoinageKeyIndex(TEST_INSTALLATION, NEXT_INDEX + 1)), indices)
    }

    @Test
    fun `a held gate does not hold the allocation mutex`() = runTest {
        val first = async(start = CoroutineStart.UNDISPATCHED) { allocator.allocate(ValueExponent(3), CoinProvenance.UNKNOWN) }
        val second = async(start = CoroutineStart.UNDISPATCHED) { allocator.allocate(ValueExponent(3), CoinProvenance.UNKNOWN) }
        runCurrent()
        assertEquals(2, gate.waitingCallers)

        gate.releaseLatestCaller(TEST_INSTALLATION)
        runCurrent()

        assertTrue(second.isCompleted)
        assertFalse(first.isCompleted)

        gate.release(TEST_INSTALLATION)
        val indices = listOf(first.await(), second.await()).map { it.getOrThrow().derivationIndex.item }.sorted()
        assertEquals(listOf(NEXT_INDEX, NEXT_INDEX + 1), indices)
    }

    @Test
    fun `an invalid exponent fails without waiting for the gate`() = runTest {
        assertFailure(allocator.allocate(ValueExponent(31), CoinProvenance.UNKNOWN))
        assertFailure(allocator.allocateAll(listOf(ValueExponent(1), ValueExponent(31)), CoinProvenance.UNKNOWN))
        assertEquals(0, gate.waitingCallers)
        coVerify(exactly = 0) { coinRepository.saveNew(any<Coin>()) }
    }

    @Test
    fun `allocateAll with no exponents waits on the gate and returns an empty list`() = runTest {
        val call = async(start = CoroutineStart.UNDISPATCHED) { allocator.allocateAll(emptyList(), CoinProvenance.UNKNOWN) }
        runCurrent()
        assertFalse(call.isCompleted)

        gate.release(TEST_INSTALLATION)

        assertEquals(Result.success(emptyList<Coin>()), call.await())
    }

    @Test
    fun `a failed installation read gives a failed result`() = runTest {
        gate.fail(IllegalStateException("installation table unreadable"))

        assertFailure(allocator.allocate(ValueExponent(3), CoinProvenance.UNKNOWN))
        assertFailure(allocator.allocateAll(listOf(ValueExponent(1)), CoinProvenance.UNKNOWN))
        coVerify(exactly = 0) { coinRepository.getNextDerivationIndex(TEST_INSTALLATION) }
    }

    @Test
    fun `a cancelled wait is rethrown, not turned into a failed result`() = runTest {
        var result: Result<Coin>? = null
        val call = launch(start = CoroutineStart.UNDISPATCHED) { result = allocator.allocate(ValueExponent(3), CoinProvenance.UNKNOWN) }
        runCurrent()

        call.cancel()
        runCurrent()

        assertEquals(null, result)
    }
    private companion object {
        const val CHAIN = "people"
        const val NEXT_INDEX = 41
    }
}
