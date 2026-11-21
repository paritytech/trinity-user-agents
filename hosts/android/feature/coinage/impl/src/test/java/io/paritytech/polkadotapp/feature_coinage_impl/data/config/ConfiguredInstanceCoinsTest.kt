package io.paritytech.polkadotapp.feature_coinage_impl.data.config

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Telling "no coins" apart from "cannot say". Both callers discard a look that returns null, so reading it as
 * an empty split would let a claim be abandoned, or a payment be called failed, on a look nobody could judge.
 */
class ConfiguredInstanceCoinsTest {
    private val coinageInstanceIdProvider: CoinageInstanceIdProvider = mockk()

    private val configuredInstanceCoins = ConfiguredInstanceCoins(coinageInstanceIdProvider)

    @Test
    fun `a look the configured instance cannot be read for is no look at all`() = runTest {
        coEvery { coinageInstanceIdProvider.instanceId() } returns Result.failure(IllegalStateException("Malformed coinage instance id: abc"))

        assertNull(configuredInstanceCoins.splitOrNull(mapOf(coin(1) to info(instance = 0))))
    }

    /** A look with nothing on it says the same whatever the instance is, so it must not depend on reading one. */
    @Test
    fun `a look with no coins needs no configured instance`() = runTest {
        val split = configuredInstanceCoins.splitOrNull(mapOf(coin(1) to null))

        assertEquals(emptyMap<AccountId, OnChainCoinInfo>(), split?.own)
        assertEquals(emptySet<AccountId>(), split?.foreign)
        coVerify(exactly = 0) { coinageInstanceIdProvider.instanceId() }
    }

    @Test
    fun `coins are split by the configured instance`() = runTest {
        val own = coin(1)
        val foreign = coin(2)
        coEvery { coinageInstanceIdProvider.instanceId() } returns Result.success(7u)

        val split = configuredInstanceCoins.splitOrNull(mapOf(own to info(instance = 7), foreign to info(instance = 0)))

        assertEquals(setOf(own), split?.own?.keys)
        assertEquals(setOf(foreign), split?.foreign)
    }

    private fun coin(seed: Int): AccountId = byteArrayOf(seed.toByte()).toDataByteArray()

    private fun info(instance: Int) = OnChainCoinInfo(instanceId = instance, value = 3, age = 0)
}
