package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinPrivateKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.deriveKeypair
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageAssetKind
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.EntryAssets
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.LedgerAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.builders.ClaimExtrinsicBuilder
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
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * What a claim contributes to being built in the background: the peer's key from its params, the coin its first
 * attempt recorded as destination, and the claim extrinsic signed with that key. When it is built is not this
 * suite's.
 */
@OptIn(ExperimentalTime::class)
class ClaimRebuildTest {
    @Before
    fun mockDerivation() = mockkStatic(DERIVATION_FILE)

    @After
    fun unmockDerivation() = unmockkStatic(DERIVATION_FILE)

    private val chain: Chain = mockk()
    private val chainAssetProvider: ChainAssetProvider = mockk()
    private val coinRepository: CoinRepository = mockk()
    private val claimExtrinsicBuilder: ClaimExtrinsicBuilder = mockk()

    private val rebuild = ClaimRebuild(chainAssetProvider, coinRepository, claimExtrinsicBuilder)

    @Before
    fun setUp() {
        every { chainAssetProvider.chainId() } returns "test-chain"
        coEvery { chainAssetProvider.chain() } returns chain
    }

    // ---- reading back ----

    /** A payment already made out of the claimed coin waits on that exact coin, so the destination is the recorded one. */
    @Test
    fun `a claim resolves to its recorded output as destination`() = runTest {
        val claim = claimOf(1)

        val resolved = rebuild.resolve(listOf(claim.scheduled), assetsOf(claim)).getValue(claim.id)

        assertEquals(claim.destination, resolved.destination)
        assertEquals(claim.source, resolved.source)
    }

    @Test
    fun `a claim without a recorded output is left out`() = runTest {
        val claim = claimOf(1)
        val assets = mapOf(claim.id to EntryAssets(inputs = listOf(claim.inputAsset()), outputs = emptyList()))

        assertTrue(rebuild.resolve(listOf(claim.scheduled), assets).isEmpty())
    }

    /** Only the params carry the peer's key; without it there is nothing to sign the claim with. */
    @Test
    fun `a claim whose params cannot be read is left out`() = runTest {
        val claim = claimOf(1)
        val unreadable = ScheduledDurableTx(
            id = claim.id,
            domainId = COINAGE_DOMAIN,
            groupId = GROUP,
            policy = SubmissionPolicy(SubmissionPolicyId(COINAGE_CLAIM_POLICY_ID), byteArrayOf(1).toDataByteArray()),
        )

        assertTrue(rebuild.resolve(listOf(unreadable), assetsOf(claim)).isEmpty())
    }

    // ---- what it waits for ----

    @Test
    fun `a claim waits on the peer's coin`() = runTest {
        val claim = claimOf(1)
        val resolved = rebuild.resolve(listOf(claim.scheduled), assetsOf(claim)).getValue(claim.id)

        assertEquals(setOf(claim.source), rebuild.inputsOf(resolved))
    }

    @Test
    fun `presence reports the peer's coins the chain holds`() = runTest {
        val present = claimOf(1).source
        val absent = claimOf(2).source
        givenChainReads(listOf(Result.success(setOf(present))))

        val looks = rebuild.presence(setOf(present, absent)).take(1).toList()

        assertEquals(listOf(setOf(present)), looks)
    }

    @Test
    fun `a failed chain read is not reported as a look`() = runTest {
        val coin = claimOf(1).source
        givenChainReads(listOf(Result.failure(IllegalStateException("node unreachable")), Result.success(emptySet())))

        val looks = rebuild.presence(setOf(coin)).take(1).toList()

        assertEquals(listOf(emptySet<AccountId>()), looks)
    }

    // ---- how long it is retried ----

    /** A peer's coin is money nothing else will collect, so a claim's failures are always weighed against its window. */
    @Test
    fun `a claim's failures are always weighed against its window`() {
        val params = claimOf(1).scheduled.policy.params

        assertEquals(RebuildTerms(RETRY_UNTIL, retriesFailures = true), rebuild.termsOf(params))
    }

    @Test
    fun `unreadable params give no terms`() {
        assertNull(rebuild.termsOf(byteArrayOf(1).toDataByteArray()))
    }

    // ---- building ----

    @Test
    fun `a rebuilt claim mints to the output already recorded in the ledger`() = runTest {
        val claim = claimOf(1)
        givenBuildsSucceed()
        val resolved = rebuild.resolve(listOf(claim.scheduled), assetsOf(claim)).getValue(claim.id)

        rebuild.build(listOf(resolved))

        coVerify(exactly = 1) { claimExtrinsicBuilder.build(chain, any(), claim.destination) }
    }

    /** The peer's key lives only in the payment message, so the rebuild signs with the one kept in the params. */
    @Test
    fun `a rebuilt claim is signed with the received key from params`() = runTest {
        val claim = claimOf(1)
        givenBuildsSucceed()
        val resolved = rebuild.resolve(listOf(claim.scheduled), assetsOf(claim)).getValue(claim.id)

        rebuild.build(listOf(resolved))

        coVerify(exactly = 1) { claimExtrinsicBuilder.build(any(), claim.keypair, any()) }
    }

    @Test
    fun `a claim that cannot be built fails the whole build`() = runTest {
        val claim = claimOf(1)
        coEvery { claimExtrinsicBuilder.build(any(), any(), any()) } returns Result.failure(IllegalStateException("no runtime"))
        val resolved = rebuild.resolve(listOf(claim.scheduled), assetsOf(claim)).getValue(claim.id)

        assertTrue(rebuild.build(listOf(resolved)).isFailure)
    }

    // ---- harness ----

    private fun givenBuildsSucceed() {
        coEvery { claimExtrinsicBuilder.build(any(), any(), any()) } returns Result.success(mockk<EnrichedSendableExtrinsic>())
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

    private class PeerClaim(
        val scheduled: ScheduledDurableTx,
        val keypair: Sr25519Keypair,
        val source: AccountId,
        val destination: AccountId,
        val outputIndex: Int,
    ) {
        val id: DurableTxId get() = scheduled.id

        fun inputAsset() = LedgerAsset(CoinageAssetKind.COIN, asset = null, publicKey = source)
    }

    /** Derivation is a top-level extension over the SDK's sr25519 factory; only the seam matters here. */
    private fun claimOf(seed: Int): PeerClaim {
        val privateKey: CoinPrivateKey = byteArrayOf(seed.toByte()).toDataByteArray()
        val keypair: Sr25519Keypair = mockk()

        every { keypair.publicKey } returns privateKey.value
        every { privateKey.deriveKeypair() } returns keypair

        val scheduled = ScheduledDurableTx(
            id = DurableTxId(seed.toLong()),
            domainId = COINAGE_DOMAIN,
            groupId = GROUP,
            policy = CoinageSubmissionParams.claimPolicy(ClaimSubmissionParams(RETRY_UNTIL, privateKey)),
        )

        return PeerClaim(
            scheduled = scheduled,
            keypair = keypair,
            source = privateKey.value.toDataByteArray(),
            destination = byteArrayOf(100, seed.toByte()).toDataByteArray(),
            outputIndex = 100 + seed,
        )
    }

    private fun assetsOf(vararg claims: PeerClaim): Map<DurableTxId, EntryAssets> = claims.associate { claim ->
        claim.id to EntryAssets(
            inputs = listOf(claim.inputAsset()),
            outputs = listOf(LedgerAsset(CoinageAssetKind.COIN, OwnAsset.Coin(testKey(claim.outputIndex)), claim.destination)),
        )
    }

    private companion object {
        val GROUP = OperationGroupId("chat-claim:1")

        val RETRY_UNTIL = Instant.fromEpochSeconds(1_000)

        const val DERIVATION_FILE = "io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferMemoKt"
    }
}
