package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropy
import io.paritytech.polkadotapp.bandersnatch_crypto.aliasInContext
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinageInstanceIdProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.FreeUnloadTokenResolver
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.UnloadTokenResolverFactory
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.createForCollection
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.RecyclerProofDataProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.context.CoinageSigningContextProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins.CoinageTransactionOrigins
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageAssetKind
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.EntryAssets
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.LedgerAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.toSplitDestinations
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.builders.UnloadExtrinsicBuilder
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.UnloadQuotaTracker
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.COINAGE_DOMAIN
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProver
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.ActivePeopleCollectionUseCase
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.OperationGroupId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.ScheduledDurableTx
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
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
import java.math.BigInteger
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * What a payment's recycler unloads contribute to being built in the background: the vouchers each one redeems
 * and the coins it mints, read back from the ledger, and every unload of one call built together. When they are
 * built is not this suite's.
 *
 * The builder is real, over mocked collaborators: its `build` takes a context parameter and returns a `Result`,
 * which mockk cannot stub, and building together is exactly what these tests are about.
 */
@OptIn(ExperimentalTime::class)
class UnloadRebuildTest {
    private val chain: Chain = mockk()
    private val peopleCollection: PeopleCollection = mockk()
    private val chainAssetProvider: ChainAssetProvider = mockk()
    private val coinRepository: CoinRepository = mockk()
    private val voucherRepository: VoucherRepository = mockk()
    private val activePeopleCollectionUseCase: ActivePeopleCollectionUseCase = mockk()

    private val chainStateRepository: ChainStateRepository = mockk()
    private val originFactory: CoinageTransactionOrigins = mockk()
    private val signingContextProvider: CoinageSigningContextProvider = mockk()
    private val voucherRingDerivation: VoucherRingDerivation = mockk()
    private val recyclerProofDataProvider: RecyclerProofDataProvider = mockk()
    private val tokenResolverFactory: UnloadTokenResolverFactory = mockk()
    private val tokenResolver: FreeUnloadTokenResolver = mockk()
    private val extrinsicService: ExtrinsicService = mockk()
    private val peopleMembershipProver: PeopleMembershipProver = mockk()
    private val quotaTracker: UnloadQuotaTracker = mockk()
    private val instanceIdProvider: CoinageInstanceIdProvider = mockk()

    private val unloadExtrinsicBuilder = UnloadExtrinsicBuilder(
        chainStateRepository = chainStateRepository,
        originFactory = originFactory,
        coinageSigningContextProvider = signingContextProvider,
        voucherRingDerivation = voucherRingDerivation,
        recyclerProofDataProvider = recyclerProofDataProvider,
        unloadTokenResolverFactory = tokenResolverFactory,
        extrinsicService = extrinsicService,
        peopleMembershipProver = peopleMembershipProver,
        quotaTracker = quotaTracker,
        coinageInstanceIdProvider = instanceIdProvider,
    )

    private val rebuild = UnloadRebuild(
        chainAssetProvider = chainAssetProvider,
        coinRepository = coinRepository,
        voucherRepository = voucherRepository,
        activePeopleCollectionUseCase = activePeopleCollectionUseCase,
        unloadExtrinsicBuilder = unloadExtrinsicBuilder,
    )

    private val knownCoins = mutableListOf<Coin>()
    private val knownVouchers = mutableListOf<RecyclerVoucher>()

    /** Every build call, as the voucher sets of the unloads it built, in the order they were built. */
    private val buildCalls = mutableListOf<MutableList<List<CoinageKeyIndex>>>()
    private val builtFor = mutableMapOf<CoinageKeyIndex, EnrichedSendableExtrinsic>()
    private val tokensUsed = mutableListOf<FreeUnloadTokenResolver.ResolvedUnloadToken>()
    private val destinationsBuilt = mutableListOf<List<Coin>>()
    private val originVouchers = mutableMapOf<TransactionOrigin, List<CoinageKeyIndex>>()

    @After
    fun unmockExtensions() {
        unmockkStatic(TOKEN_RESOLVER_FILE, ALIAS_FILE, DESTINATIONS_FILE)
    }

    @Before
    fun setUp() {
        // Before any stub below: the builder's collaborators include top-level extensions.
        mockkStatic(TOKEN_RESOLVER_FILE, ALIAS_FILE, DESTINATIONS_FILE)
        every { chainAssetProvider.chainId() } returns "test-chain"
        coEvery { chainAssetProvider.chain() } returns chain
        coEvery { activePeopleCollectionUseCase.getActivePeopleCollection() } returns peopleCollection
        coEvery { coinRepository.getCoinsBy(any()) } answers {
            val requested = firstArg<List<CoinageKeyIndex>>()
            knownCoins.filter { it.derivationIndex in requested }
        }
        coEvery { voucherRepository.getByRingVrfKeyIndices(any()) } answers {
            val requested = firstArg<List<CoinageKeyIndex>>()
            knownVouchers.filter { it.ringVrfKeyIndex in requested }
        }
        givenBuilderCollaborators()
        givenBuildsSucceed()
    }

    // ---- reading back ----

    @Test
    fun `an unload resolves to its recorded vouchers and outputs, in order`() = runTest {
        val unload = unloadOf(1)
        knownCoins.reverse()
        knownVouchers.reverse()

        val resolved = rebuild.resolve(listOf(unload.scheduled), assetsOf(unload)).getValue(unload.id)

        assertEquals(unload.voucherIndices, resolved.voucherIndices)
        assertEquals(unload.outputCoins, resolved.outputs)
    }

    @Test
    fun `an unload with a voucher not known locally is left out`() = runTest {
        val unload = unloadOf(1)
        knownVouchers -= unload.vouchers.last()

        assertTrue(rebuild.resolve(listOf(unload.scheduled), assetsOf(unload)).isEmpty())
    }

    @Test
    fun `an unload with an output not known locally is left out`() = runTest {
        val unload = unloadOf(1)
        knownCoins -= unload.outputCoins.last()

        assertTrue(rebuild.resolve(listOf(unload.scheduled), assetsOf(unload)).isEmpty())
    }

    /** An unload redeems vouchers only; a coin recorded as its input cannot be this kind of transaction. */
    @Test
    fun `an unload recorded with an input that is not a voucher is left out`() = runTest {
        val unload = unloadOf(1)
        val coin = unload.outputCoins.first()
        val assets = mapOf(
            unload.id to EntryAssets(
                inputs = listOf(LedgerAsset(CoinageAssetKind.COIN, OwnAsset.Coin(coin.derivationIndex), coin.accountId)),
                outputs = unload.outputCoins.map { it.asLedgerAsset() },
            )
        )

        assertTrue(rebuild.resolve(listOf(unload.scheduled), assets).isEmpty())
    }

    @Test
    fun `an unload with no recorded vouchers is left out`() = runTest {
        val unload = unloadOf(1)
        val assets = mapOf(unload.id to EntryAssets(inputs = emptyList(), outputs = unload.outputCoins.map { it.asLedgerAsset() }))

        assertTrue(rebuild.resolve(listOf(unload.scheduled), assets).isEmpty())
    }

    // ---- what it waits for ----

    @Test
    fun `an unload waits on every voucher it redeems`() = runTest {
        val unload = unloadOf(1)
        val resolved = rebuild.resolve(listOf(unload.scheduled), assetsOf(unload)).getValue(unload.id)

        assertEquals(unload.voucherIndices.toSet(), rebuild.inputsOf(resolved))
    }

    /** A voucher is present while it sits in a recycler: that is where an unload proves it. */
    @Test
    fun `presence reports the vouchers sitting in a recycler`() = runTest {
        val unload = unloadOf(1)
        every { voucherRepository.subscribeVouchersInRecycler() } returns flow {
            emit(listOf(unload.vouchers.first()))
            awaitCancellation()
        }

        val looks = rebuild.presence(unload.voucherIndices.toSet()).take(1).toList()

        assertEquals(listOf(setOf(unload.voucherIndices.first())), looks)
    }

    // ---- how long it is retried ----

    @Test
    fun `terms carry the build deadline and whether failures are retried`() {
        val params = CoinageSubmissionParams.unloadPolicy(TransferSubmissionParams(RETRY_UNTIL, retryFailures = true)).params

        assertEquals(RebuildTerms(RETRY_UNTIL, retriesFailures = true), rebuild.termsOf(params))
    }

    @Test
    fun `unreadable params give no terms`() {
        assertNull(rebuild.termsOf(byteArrayOf(1).toDataByteArray()))
    }

    // ---- building ----

    /** The recipient holds the keys of exactly these coins, and destinations are positional. */
    @Test
    fun `a rebuild mints to exactly the outputs recorded in the ledger, in order`() = runTest {
        val unload = unloadOf(1)
        knownCoins.reverse()
        knownVouchers.reverse()

        build(unload)

        assertEquals(listOf(unload.outputCoins), destinationsBuilt)
        assertEquals(listOf(listOf(unload.voucherIndices)), buildCalls)
    }

    @Test
    fun `every unload is built, in the order given`() = runTest {
        val first = unloadOf(1)
        val second = unloadOf(2)

        val extrinsics = build(second, first).getOrThrow()

        assertEquals(listOf(builtFor[second.voucherIndices.first()], builtFor[first.voucherIndices.first()]), extrinsics)
    }

    @Test
    fun `an unload that cannot be built fails the whole build`() = runTest {
        val unload = unloadOf(1)
        givenBuildFails()

        assertTrue(build(unload).isFailure)
    }

    // ---- building together ----

    /**
     * The pinned block, the person proof and the token resolution are paid once per call, so every unload built in
     * one call is built on them rather than each paying for its own.
     */
    @Test
    fun `one pinned block, one prover and one token resolution per call`() = runTest {
        val first = unloadOf(1)
        val second = unloadOf(2)

        build(first, second)

        coVerify(exactly = 1) { chainStateRepository.currentBlockHash(any()) }
        coVerify(exactly = 1) { peopleMembershipProver.precomputeForMember(any(), any(), any()) }
        coVerify(exactly = 1) { tokenResolver.resolve(any(), 2) }
    }

    /** Two unloads spending one free token would let only one of them land, so each gets its own. */
    @Test
    fun `every entry in a call gets a distinct free unload token`() = runTest {
        val first = unloadOf(1)
        val second = unloadOf(2)

        build(first, second)

        assertEquals(2, tokensUsed.size)
        assertEquals(2, tokensUsed.map { it.counter }.distinct().size)
    }

    /** A token picked for a transaction that was never built is still there to be picked again. */
    @Test
    fun `the unload quota is noted only after a build succeeds`() = runTest {
        val unload = unloadOf(1)
        givenBuildFails()

        build(unload)

        coVerify(exactly = 0) { quotaTracker.noteUnloadsHappened(any()) }

        givenBuildsSucceed()
        build(unload)

        coVerify(exactly = 1) { quotaTracker.noteUnloadsHappened(1) }
    }

    // ---- harness ----

    private suspend fun build(vararg unloads: ScheduledUnload): Result<List<EnrichedSendableExtrinsic>> {
        val resolved = rebuild.resolve(unloads.map { it.scheduled }, assetsOf(*unloads))

        return rebuild.build(unloads.map { resolved.getValue(it.id) })
    }

    private fun givenBuilderCollaborators() {
        every { chain.id } returns "test-chain"
        coEvery { chainStateRepository.currentBlockHash(any()) } returns "0xpinned"
        coEvery { recyclerProofDataProvider.getRecyclerRevisions(any(), any(), any()) } answers {
            Result.success(secondArg<Collection<RecyclerKey>>().associateWith { RingRevision(1) })
        }
        every { tokenResolverFactory.createForCollection(any()) } returns tokenResolver
        coEvery { tokenResolver.resolve(any(), any()) } answers {
            buildCalls += mutableListOf<List<CoinageKeyIndex>>()
            List(secondArg<Int>()) {
                FreeUnloadTokenResolver.ResolvedUnloadToken(period = 1, counter = it.toLong(), unloadTokenContext = BandersnatchContext(byteArrayOf(9)))
            }
        }
        coEvery { peopleMembershipProver.precomputeForMember(any(), any(), any()) } returns Result.success(mockk())
        coEvery { instanceIdProvider.instanceId() } returns Result.success(1u)
        every { signingContextProvider.recyclerVouchersContext() } returns BandersnatchContext(byteArrayOf(8))
        coEvery { voucherRingDerivation.deriveBandersnatch(any()) } returns BandersnatchEntropy(byteArrayOf(7))
        every { any<BandersnatchEntropy>().aliasInContext(any()) } returns BandersnatchAlias(byteArrayOf(6))
        every { any<List<Coin>>().toSplitDestinations() } answers {
            // A static extension's receiver arrives as its first argument.
            destinationsBuilt += firstArg<List<Coin>>()
            emptyList()
        }
        coEvery { quotaTracker.noteUnloadsHappened(any()) } returns Unit
        every { originFactory.createAsUnloadTokenPeopleOrigin(any(), any(), any(), any(), any()) } answers {
            val origin: TransactionOrigin = mockk()
            originVouchers[origin] = secondArg<List<RecyclerVoucher>>().map { it.ringVrfKeyIndex }
            tokensUsed += thirdArg<FreeUnloadTokenResolver.ResolvedUnloadToken>()
            origin
        }
    }

    private fun givenBuildsSucceed() {
        coEvery { extrinsicService.buildExtrinsic(any(), any(), any(), any()) } answers {
            val vouchers = originVouchers.getValue(secondArg())
            buildCalls.last() += vouchers

            mockk<EnrichedSendableExtrinsic>()
                .also { builtFor[vouchers.first()] = it }
                .let { Result.success(it) }
        }
    }

    private fun givenBuildFails() {
        coEvery { extrinsicService.buildExtrinsic(any(), any(), any(), any()) } returns
            Result.failure(IllegalStateException("no ring revision"))
    }

    private class ScheduledUnload(
        val scheduled: ScheduledDurableTx,
        val vouchers: List<RecyclerVoucher>,
        val outputCoins: List<Coin>,
    ) {
        val id: DurableTxId get() = scheduled.id
        val voucherIndices: List<CoinageKeyIndex> get() = vouchers.map { it.ringVrfKeyIndex }
    }

    /** Two vouchers unloaded into two coins; a seed keeps different unloads' assets apart. */
    private fun unloadOf(seed: Int): ScheduledUnload {
        val vouchers = listOf(voucherOf(seed * 10), voucherOf(seed * 10 + 1))
        val outputs = listOf(coinOf(seed * 10 + 2), coinOf(seed * 10 + 3))
        knownVouchers += vouchers
        knownCoins += outputs

        val scheduled = ScheduledDurableTx(
            id = DurableTxId(seed.toLong()),
            domainId = COINAGE_DOMAIN,
            groupId = GROUP,
            policy = CoinageSubmissionParams.unloadPolicy(TransferSubmissionParams(RETRY_UNTIL, retryFailures = true)),
        )

        return ScheduledUnload(scheduled, vouchers, outputs)
    }

    private fun voucherOf(item: Int): RecyclerVoucher = mockk<RecyclerVoucher>().also {
        every { it.ringVrfKeyIndex } returns testKey(item)
        every { it.recyclerValue } returns ValueExponent(3)
        every { it.location } returns RecyclerVoucher.Location.InRecycler(
            recyclerIndex = RingIndex(BigInteger.ZERO),
            recyclerMembers = 10,
            enteredAt = null,
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

    private fun assetsOf(vararg unloads: ScheduledUnload): Map<DurableTxId, EntryAssets> = unloads.associate { unload ->
        unload.id to EntryAssets(
            inputs = unload.voucherIndices.map { index ->
                LedgerAsset(CoinageAssetKind.VOUCHER, OwnAsset.Voucher(index), byteArrayOf(index.item.toByte(), 1).toDataByteArray())
            },
            outputs = unload.outputCoins.map { it.asLedgerAsset() },
        )
    }

    private companion object {
        const val TOKEN_RESOLVER_FILE = "io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.FreeUnloadTokenResolverKt"
        const val ALIAS_FILE = "io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropyKt"
        const val DESTINATIONS_FILE = "io.paritytech.polkadotapp.feature_coinage_impl.domain.model.SplitDestinationKt"

        val GROUP = OperationGroupId("chat-send")

        val RETRY_UNTIL = Instant.fromEpochSeconds(1_000)
    }
}
