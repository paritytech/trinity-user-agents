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
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.CoinageInstallationRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.ExponentBoundsRepository
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class RealCoinAllocatorTest {
    private val coinRepository = mockk<CoinRepository>(relaxed = true)

    private val allocator = RealCoinAllocator(
        coinRepository = coinRepository,
        installationRepository = mockk<CoinageInstallationRepository> { coEvery { getOrCreateCurrent() } returns TEST_INSTALLATION },
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
        coEvery { coinRepository.getNextDerivationIndex(TEST_INSTALLATION) } returns NEXT_INDEX
    }

    @Test
    fun `an allocated coin takes the next index of the current installation`() = runTest {
        val coin = allocator.allocate(ValueExponent(3), CoinProvenance.UNKNOWN).getOrThrow()

        assertEquals(CoinageKeyIndex(TEST_INSTALLATION, NEXT_INDEX), coin.derivationIndex)
        coVerify { coinRepository.saveNew(coin) }
    }

    @Test
    fun `an allocation whose index is already taken fails instead of overwriting the row`() = runTest {
        coEvery { coinRepository.saveNew(any<Coin>()) } throws IllegalStateException("UNIQUE constraint failed: coins")

        val result = allocator.allocate(ValueExponent(3), CoinProvenance.UNKNOWN)

        assertTrue(result.isFailure)
        coVerify(exactly = 0) { coinRepository.save(any()) }
    }

    @Test
    fun `a batch whose indices are already taken fails instead of overwriting the rows`() = runTest {
        coEvery { coinRepository.saveNew(any<List<Coin>>()) } throws IllegalStateException("UNIQUE constraint failed: coins")

        val result = allocator.allocateAll(listOf(ValueExponent(1), ValueExponent(2)), CoinProvenance.UNKNOWN)

        assertTrue(result.isFailure)
        coVerify(exactly = 0) { coinRepository.saveAll(any()) }
    }

    private companion object {
        const val CHAIN = "people"
        const val NEXT_INDEX = 41
    }
}
