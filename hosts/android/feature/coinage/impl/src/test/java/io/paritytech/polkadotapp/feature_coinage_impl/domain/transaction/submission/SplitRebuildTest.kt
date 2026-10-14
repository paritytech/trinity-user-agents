package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageAssetKind
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.EntryAssets
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.LedgerAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.builders.SplitExtrinsicBuilder
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.COINAGE_DOMAIN
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.OperationGroupId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.ScheduledDurableTx
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicyId
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * What a payment's `Coinage.split` contributes to being built in the background: the coin it spends and the coins
 * it mints, read back from the ledger, and the split extrinsic over them. When it is built is not this suite's.
 */
@OptIn(ExperimentalTime::class)
class SplitRebuildTest {
    private val chain: Chain = mockk()
    private val chainAssetProvider: ChainAssetProvider = mockk()
    private val coinRepository: CoinRepository = mockk()
    private val splitExtrinsicBuilder: SplitExtrinsicBuilder = mockk()

    private val rebuild = SplitRebuild(chainAssetProvider, coinRepository, splitExtrinsicBuilder)

    private val knownCoins = mutableListOf<Coin>()

    @Before
    fun setUp() {
        every { chainAssetProvider.chainId() } returns "test-chain"
        coEvery { chainAssetProvider.chain() } returns chain
        coEvery { coinRepository.getCoinsBy(any()) } answers {
            val requested = firstArg<List<CoinageKeyIndex>>()
            knownCoins.filter { it.derivationIndex in requested }
        }
    }

    // ---- reading back ----

    /** Destinations are positional, so the outputs come back in ledger order whatever order the coin table uses. */
    @Test
    fun `a split resolves to its recorded coin and outputs, in order`() = runTest {
        val split = splitOf(1)
        knownCoins.reverse()

        val resolved = rebuild.resolve(listOf(split.scheduled), assetsOf(split)).getValue(split.id)

        assertEquals(split.inputCoin, resolved.coinToSplit)
        assertEquals(split.outputCoins, resolved.outputs)
    }

    @Test
    fun `a split whose coin is not known locally is left out`() = runTest {
        val split = splitOf(1)
        knownCoins -= split.inputCoin

        assertTrue(rebuild.resolve(listOf(split.scheduled), assetsOf(split)).isEmpty())
    }

    @Test
    fun `a split with an output not known locally is left out`() = runTest {
        val split = splitOf(1)
        knownCoins -= split.outputCoins.last()

        assertTrue(rebuild.resolve(listOf(split.scheduled), assetsOf(split)).isEmpty())
    }

    @Test
    fun `a split the ledger holds nothing for is left out`() = runTest {
        val split = splitOf(1)

        assertTrue(rebuild.resolve(listOf(split.scheduled), emptyMap()).isEmpty())
    }

    /** A split spends exactly one coin; anything else recorded as its input cannot be this kind of transaction. */
    @Test
    fun `a split recorded with more than one input is left out`() = runTest {
        val split = splitOf(1)
        val other = coinOf(99).also { knownCoins += it }
        val assets = mapOf(
            split.id to EntryAssets(
                inputs = listOf(split.inputCoin.asLedgerAsset(), other.asLedgerAsset()),
                outputs = split.outputCoins.map { it.asLedgerAsset() },
            )
        )

        assertTrue(rebuild.resolve(listOf(split.scheduled), assets).isEmpty())
    }

    // ---- what it waits for ----

    @Test
    fun `a split waits on the coin it spends`() = runTest {
        val split = splitOf(1)
        val resolved = rebuild.resolve(listOf(split.scheduled), assetsOf(split)).getValue(split.id)

        assertEquals(setOf(split.inputCoin.accountId), rebuild.inputsOf(resolved))
    }

    @Test
    fun `presence reports the coins the chain holds`() = runTest {
        val present = coinOf(1).accountId
        val absent = coinOf(2).accountId
        givenChainReads(listOf(Result.success(setOf(present))))

        val looks = rebuild.presence(setOf(present, absent)).take(1).toList()

        assertEquals(listOf(setOf(present)), looks)
    }

    /** A read that could not be taken says nothing about the chain, so it is not reported as a look at all. */
    @Test
    fun `a failed chain read is not reported as a look`() = runTest {
        val coin = coinOf(1).accountId
        givenChainReads(listOf(Result.failure(IllegalStateException("node unreachable")), Result.success(emptySet())))

        val looks = rebuild.presence(setOf(coin)).take(1).toList()

        assertEquals(listOf(emptySet<AccountId>()), looks)
    }

    // ---- how long it is retried ----

    @Test
    fun `terms carry the build deadline and whether failures are retried`() {
        val params = CoinageSubmissionParams.splitPolicy(TransferSubmissionParams(RETRY_UNTIL, retryFailures = false)).params

        assertEquals(RebuildTerms(RETRY_UNTIL, retriesFailures = false), rebuild.termsOf(params))
    }

    @Test
    fun `unreadable params give no terms`() {
        assertNull(rebuild.termsOf(byteArrayOf(1).toDataByteArray()))
    }

    // ---- building ----

    /** The recipient holds the keys of exactly these coins, so a rebuild splits into them and nothing else. */
    @Test
    fun `a rebuild mints to exactly the outputs recorded in the ledger, in order`() = runTest {
        val split = splitOf(1)
        knownCoins.reverse()
        givenBuildsSucceed()
        val resolved = rebuild.resolve(listOf(split.scheduled), assetsOf(split)).getValue(split.id)

        rebuild.build(listOf(resolved))

        coVerify(exactly = 1) { splitExtrinsicBuilder.build(chain, split.inputCoin, split.outputCoins) }
    }

    @Test
    fun `every split is built, in the order given`() = runTest {
        val first = splitOf(1)
        val second = splitOf(2)
        val built = givenBuildsSucceed()
        val resolved = rebuild.resolve(listOf(first.scheduled, second.scheduled), assetsOf(first, second))

        val extrinsics = rebuild.build(listOf(resolved.getValue(second.id), resolved.getValue(first.id))).getOrThrow()

        assertEquals(listOf(built.getValue(second.inputCoin), built.getValue(first.inputCoin)), extrinsics)
    }

    @Test
    fun `a split that cannot be built fails the whole build`() = runTest {
        val split = splitOf(1)
        coEvery { splitExtrinsicBuilder.build(any(), any(), any()) } returns Result.failure(IllegalStateException("no runtime"))
        val resolved = rebuild.resolve(listOf(split.scheduled), assetsOf(split)).getValue(split.id)

        assertTrue(rebuild.build(listOf(resolved)).isFailure)
    }

    // ---- harness ----

    private fun givenBuildsSucceed(): Map<Coin, EnrichedSendableExtrinsic> {
        val built = mutableMapOf<Coin, EnrichedSendableExtrinsic>()

        coEvery { splitExtrinsicBuilder.build(any(), any(), any()) } answers {
            Result.success(mockk<EnrichedSendableExtrinsic>().also { built[secondArg()] = it })
        }

        return built
    }

    /** One emission per read, and a subscription that stays open after the last one. */
    private fun givenChainReads(reads: List<Result<Set<AccountId>>>) {
        coEvery { coinRepository.subscribeCoinsInfoFor(any(), any()) } answers {
            val requested = secondArg<List<AccountId>>()

            flow {
                reads.forEach { read ->
                    emit(
                        read.map { present ->
                            requested.associateWith { OnChainCoinInfo(instanceId = 0, value = 3, age = 0).takeIf { _ -> it in present } }
                        }
                    )
                }
                awaitCancellation()
            }
        }
    }

    private class ScheduledSplit(val scheduled: ScheduledDurableTx, val inputCoin: Coin, val outputCoins: List<Coin>) {
        val id: DurableTxId get() = scheduled.id
    }

    /** One coin split into two; a seed keeps different splits' coins apart. */
    private fun splitOf(seed: Int): ScheduledSplit {
        val input = coinOf(seed * 10)
        val outputs = listOf(coinOf(seed * 10 + 1), coinOf(seed * 10 + 2))
        knownCoins += input
        knownCoins += outputs

        val scheduled = ScheduledDurableTx(
            id = DurableTxId(seed.toLong()),
            domainId = COINAGE_DOMAIN,
            groupId = GROUP,
            policy = SubmissionPolicy(SubmissionPolicyId(COINAGE_SPLIT_POLICY_ID), byteArrayOf().toDataByteArray()),
        )

        return ScheduledSplit(scheduled, input, outputs)
    }

    private fun assetsOf(vararg splits: ScheduledSplit): Map<DurableTxId, EntryAssets> = splits.associate { split ->
        split.id to EntryAssets(
            inputs = listOf(split.inputCoin.asLedgerAsset()),
            outputs = split.outputCoins.map { it.asLedgerAsset() },
        )
    }

    private fun coinOf(item: Int) = Coin(
        derivationIndex = testKey(item),
        valueExponent = ValueExponent(3),
        age = Coin.Age.Unknown,
        isOnChain = false,
        accountId = byteArrayOf(item.toByte()).toDataByteArray(),
        provenance = CoinProvenance.UNKNOWN,
    )

    private fun Coin.asLedgerAsset() = LedgerAsset(CoinageAssetKind.COIN, OwnAsset.Coin(derivationIndex), accountId)

    private companion object {
        val GROUP = OperationGroupId("chat-send")

        val RETRY_UNTIL = Instant.fromEpochSeconds(1_000)
    }
}
