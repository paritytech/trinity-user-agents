package io.paritytech.polkadotapp.feature_coinage_impl.domain.service

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinUpdate
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Hop
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Keeping each coin's on-chain presence current.
 *
 * Two things make this delicate. Its writes land in the `coins` table, which Room invalidates as a whole, so
 * a needless write re-runs every join in the app that reads a coin. And the age it writes is the only record
 * that a coin was ever on chain at all — clearing it would make a coin the peer took indistinguishable from
 * one nothing has looked at.
 */
class CoinPresenceSyncServiceTest {
    private val chainAssetProvider: ChainAssetProvider = mockk()
    private val coinRepository: CoinRepository = mockk()

    private val service = CoinPresenceSyncService(chainAssetProvider, coinRepository)

    private val written = mutableListOf<List<CoinUpdate>>()
    private val hopsWritten = mutableListOf<Map<CoinageKeyIndex, List<Hop>>>()
    private val scope = CoroutineScope(UnconfinedTestDispatcher())

    @After
    fun stop() = scope.cancel()

    /**
     * A coin the chain no longer holds, already recorded as gone. Nothing about it has changed, so writing
     * it again only invalidates the table for every other reader.
     *
     * Its age is what makes it distinguishable from a coin never seen, so it is carried forward rather than
     * re-sent, which is precisely why the update looks like a change when it is not.
     */
    @Test
    fun `a coin that is still absent is not written again`() = runTest {
        givenCoins(coinOf(age = 5, onChain = false))
        givenChainHolds(emptyMap())

        startSync()

        assertTrue("expected no write, got $written", written.all { it.isEmpty() })
    }

    @Test
    fun `a coin that is still on chain at the same age is not written again`() = runTest {
        givenCoins(coinOf(age = 5, onChain = true))
        givenChainHolds(mapOf(ACCOUNT to OnChainCoinInfo(instanceId = 0, value = 3, age = 5)))

        startSync()

        assertTrue("expected no write, got $written", written.all { it.isEmpty() })
    }

    /** The coin has gone from the chain: presence drops, and no age is sent so the recorded one stands. */
    @Test
    fun `a coin that has left the chain is recorded as gone without losing its age`() = runTest {
        givenCoins(coinOf(age = 5, onChain = true))
        givenChainHolds(emptyMap())

        startSync()

        val update = written.flatten().single()
        assertEquals(false, update.onChain)
        assertEquals(null, update.age)
    }

    @Test
    fun `a coin that has appeared is recorded with the age the chain gave`() = runTest {
        givenCoins(coinOf(age = null, onChain = false))
        givenChainHolds(mapOf(ACCOUNT to OnChainCoinInfo(instanceId = 0, value = 3, age = 2)))

        startSync()

        val update = written.flatten().single()
        assertEquals(true, update.onChain)
        assertEquals(2, update.age)
    }

    @Test
    fun `a coin whose age has moved on is written with the new one`() = runTest {
        givenCoins(coinOf(age = 5, onChain = true))
        givenChainHolds(mapOf(ACCOUNT to OnChainCoinInfo(instanceId = 0, value = 3, age = 6)))

        startSync()

        assertEquals(6, written.flatten().single().age)
    }

    /**
     * A claimed coin cannot know its history when it arrives: its age lives on chain and shows up here, so
     * this is the first moment the history can be written at all.
     *
     * Every hop but the newest stands alone, because the chain reports how many times the coin moved and
     * never what moved with it. Only the last one has a crowd we recorded — the batch it was claimed in.
     */
    @Test
    fun `a claimed coin gets one hop per age with the arriving batch as the newest`() = runTest {
        givenCoins(coinOf(age = null, onChain = false, provenance = CoinProvenance.incoming(bundleSize = 4)))
        givenChainHolds(mapOf(ACCOUNT to OnChainCoinInfo(instanceId = 0, value = 3, age = 3)))

        startSync()

        assertEquals(
            listOf(Hop.Transfer.of(bundleSize = 1), Hop.Transfer.of(bundleSize = 1), Hop.Transfer.of(bundleSize = 4)),
            hopsWritten.flatMap { it.values }.single()
        )
    }

    /** A split's change coin inherits a real history; rebuilding it from the age would throw that away. */
    @Test
    fun `a coin that already knows where it came from keeps its hops`() = runTest {
        val fromRecycler = CoinProvenance.fromRecycler(RecyclerFungibility.ofPercent(80))
        givenCoins(coinOf(age = 5, onChain = true, provenance = fromRecycler))
        givenChainHolds(mapOf(ACCOUNT to OnChainCoinInfo(instanceId = 0, value = 3, age = 5)))

        startSync()

        assertTrue("expected no hop write, got $hopsWritten", hopsWritten.all { it.isEmpty() })
    }

    /** Age zero would freeze an empty history; leaving it unobserved lets a later tick fill it in. */
    @Test
    fun `a coin the chain reports at age zero has its history left open`() = runTest {
        givenCoins(coinOf(age = null, onChain = false))
        givenChainHolds(mapOf(ACCOUNT to OnChainCoinInfo(instanceId = 0, value = 3, age = 0)))

        startSync()

        assertTrue("expected no hop write, got $hopsWritten", hopsWritten.all { it.isEmpty() })
    }

    private suspend fun startSync() {
        with(ComputationalScope(scope)) { service.start() }
    }

    private fun givenCoins(vararg coins: Coin) {
        val asset: Chain.Asset = mockk()
        every { asset.chainId } returns "test-chain"
        coEvery { chainAssetProvider.asset() } returns asset

        every { coinRepository.subscribeAllCoins() } returns flowOf(coins.toList())
        coEvery { coinRepository.updateCoins(any()) } answers { written += firstArg<List<CoinUpdate>>() }
        coEvery { coinRepository.updateCoinHops(any()) } answers {
            hopsWritten += firstArg<Map<CoinageKeyIndex, List<Hop>>>()
        }
    }

    private fun givenChainHolds(coins: Map<AccountId, OnChainCoinInfo>) {
        coEvery { coinRepository.subscribeCoinsInfoFor(any(), any()) } answers {
            val requested = secondArg<List<AccountId>>()

            flowOf(Result.success(requested.associateWith { coins[it] }))
        }
    }

    private fun coinOf(
        age: Int?,
        onChain: Boolean,
        provenance: CoinProvenance = CoinProvenance.UNKNOWN
    ) = Coin(
        derivationIndex = testKey(0),
        valueExponent = ValueExponent(3),
        age = age?.let(Coin.Age::Known) ?: Coin.Age.Unknown,
        isOnChain = onChain,
        accountId = ACCOUNT,
        provenance = provenance,
    )

    private companion object {
        val ACCOUNT: AccountId = byteArrayOf(7).toDataByteArray()
    }
}
