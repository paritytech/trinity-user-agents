package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.usecase

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropy
import io.paritytech.polkadotapp.bandersnatch_crypto.aliasInContext
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher.Location
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.recyclerLocationOrThrow
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.PowerOfTwoConversion
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinageInstanceIdProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.FreeUnloadTokenResolver
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.UnloadTokenResolverFactory
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.RecyclerProofDataProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.context.CoinageSigningContextProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins.CoinageTransactionOrigins
import io.paritytech.polkadotapp.feature_coinage_impl.planks
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProver
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.ActivePeopleCollectionUseCase
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FAILURE
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING_SUCCESS
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.math.BigInteger

/**
 * Unloading recycler vouchers into someone else's balance.
 *
 * An unload is several transactions and one promise: the destination either got what it was told it would
 * get, or it got part of it, and the caller must be able to tell those apart without knowing how many
 * transactions were involved.
 *
 * Everything an unload consults before it can build a call is mocked, including the bandersnatch alias
 * derivation, which is a native call and would otherwise be the one thing keeping this path out of a unit
 * test. Only a surplus of zero is covered: folding a surplus back into freshly minted vouchers goes through
 * the balance conversion context, which is a different subject.
 */
class RealUnloadRecyclerIntoExternalAssetUseCaseTest {
    private val transactionService: CoinageTransactionService = mockk()
    private val chainRegistry: ChainRegistry = mockk()
    private val chainAssetProvider: ChainAssetProvider = mockk()
    private val rpcCalls: RpcCalls = mockk()
    private val recyclerProofDataProvider: RecyclerProofDataProvider = mockk()
    private val activePeopleCollectionUseCase: ActivePeopleCollectionUseCase = mockk()
    private val unloadTokenResolverFactory: UnloadTokenResolverFactory = mockk()
    private val coinageSigningContextProvider: CoinageSigningContextProvider = mockk()
    private val voucherRingDerivation: VoucherRingDerivation = mockk()
    private val originFactory: CoinageTransactionOrigins = mockk(relaxed = true)
    private val extrinsicService: ExtrinsicService = mockk()
    private val coinageBalanceConverterUseCase: CoinageBalanceConverterUseCase = mockk()
    private val peopleMembershipProver: PeopleMembershipProver = mockk()
    private val voucherRepository: VoucherRepository = mockk(relaxed = true)

    private val coinageInstanceIdProvider: CoinageInstanceIdProvider = mockk {
        coEvery { instanceId() } returns Result.success(0u)
    }

    private val useCase = RealUnloadRecyclerIntoExternalAssetUseCase(
        rpcCalls = rpcCalls,
        extrinsicService = extrinsicService,
        originFactory = originFactory,
        coinageSigningContextProvider = coinageSigningContextProvider,
        voucherRingDerivation = voucherRingDerivation,
        recyclerProofDataProvider = recyclerProofDataProvider,
        activePeopleCollectionUseCase = activePeopleCollectionUseCase,
        unloadTokenResolverFactory = unloadTokenResolverFactory,
        chainRegistry = chainRegistry,
        voucherRepository = voucherRepository,
        transactionService = transactionService,
        voucherAllocator = mockk(relaxed = true),
        coinAmountBreakdownUseCase = mockk(relaxed = true),
        coinageBalanceConverterUseCase = coinageBalanceConverterUseCase,
        peopleMembershipProver = peopleMembershipProver,
        quotaTracker = mockk(relaxed = true),
        chainAssetProvider = chainAssetProvider,
        coinageInstanceIdProvider = coinageInstanceIdProvider,
    )

    /** The alias each voucher signs with is a native bandersnatch call; only the seam matters here. */
    @Before
    fun mockAliasDerivation() = mockkStatic(BANDERSNATCH_FILE)

    @After
    fun unmockAliasDerivation() = unmockkStatic(BANDERSNATCH_FILE)

    private val groupId = CoinageOperationGroupId("unload")
    private val destination = byteArrayOf(1).intoAccountId()

    @Test
    fun `an unload of nothing is refused`() = runBlocking<Unit> {
        givenNothingSubmittedYet()

        val result = useCase.initiateUnload(emptyList(), destination, NO_SURPLUS, groupId)

        assertTrue(result.isFailure)
    }

    /**
     * A voucher that is not in a recycler has no recycler index to unload from, so the call cannot be built
     * at all. Refusing here keeps that from becoming a half-built extrinsic later.
     */
    @Test
    fun `a voucher that is not in a recycler is refused`() = runBlocking<Unit> {
        val vouchers = listOf(voucherInRecycler(1), voucherOf(2, Location.Onboarding))
        givenNothingSubmittedYet()

        val result = useCase.initiateUnload(vouchers, destination, NO_SURPLUS, groupId)

        assertTrue(result.isFailure)
    }

    /**
     * The app is killed between registering the unload and the caller learning of it, and the caller retries.
     * The group already holds the entries the first attempt registered.
     * Nothing is submitted again — the vouchers would be unloaded twice.
     */
    @Test
    fun `a group an earlier attempt already submitted is not unloaded again`() = runBlocking<Unit> {
        coEvery { transactionService.getOperationGroupStatuses(groupId) } returns
            Result.success(listOf(entry(PENDING)))

        val result = useCase.initiateUnload(listOf(voucherInRecycler(1)), destination, NO_SURPLUS, groupId)

        assertTrue(result.isSuccess)
        coVerify(exactly = 0) { transactionService.submitTransactions(any(), any()) }
    }

    @Test
    fun `a ledger that cannot be read fails the unload rather than resubmitting it`() = runBlocking<Unit> {
        coEvery { transactionService.getOperationGroupStatuses(groupId) } returns
            Result.failure(IllegalStateException("no database"))

        val result = useCase.initiateUnload(listOf(voucherInRecycler(1)), destination, NO_SURPLUS, groupId)

        assertTrue(result.isFailure)
        coVerify(exactly = 0) { transactionService.submitTransactions(any(), any()) }
    }

    // ---- submission ----

    /**
     * Three vouchers sit in two recyclers: two in one, one in another.
     * Each recycler is one call on chain, so the unload is two transactions.
     * Both are registered together, under the caller's group, each consuming exactly its own vouchers.
     */
    @Test
    fun `an unload registers one transaction per recycler under one group`() = runBlocking<Unit> {
        val shared = listOf(voucherInRecycler(1, recycler = 1), voucherInRecycler(2, recycler = 1))
        val alone = voucherInRecycler(3, recycler = 2)
        givenChainReadyFor(shared + alone)

        val result = useCase.initiateUnload(shared + alone, destination, NO_SURPLUS, groupId)

        assertTrue("unload failed: ${result.exceptionOrNull()}", result.isSuccess)
        assertEquals(2, registered.size)
        assertEquals(
            listOf(
                shared.map { CoinageInput.Voucher(it.ringVrfKeyIndex) },
                listOf(CoinageInput.Voucher(alone.ringVrfKeyIndex)),
            ),
            registered.map { it.inputs },
        )
    }

    /** Nothing is minted when no surplus is folded back in, so the unload claims no outputs of ours. */
    @Test
    fun `an unload with no surplus mints nothing`() = runBlocking<Unit> {
        val vouchers = listOf(voucherInRecycler(1, recycler = 1))
        givenChainReadyFor(vouchers)

        useCase.initiateUnload(vouchers, destination, NO_SURPLUS, groupId)

        assertEquals(listOf(emptyList<OwnAsset>()), registered.map { it.outputs })
    }

    /**
     * One of the two extrinsics cannot be built.
     * Neither is registered — a caller reading the group back as one outcome cannot tell half a
     * registration from half an execution, and those mean opposite things.
     */
    @Test
    fun `an extrinsic that cannot be built registers nothing at all`() = runBlocking<Unit> {
        val vouchers = listOf(voucherInRecycler(1, recycler = 1), voucherInRecycler(2, recycler = 2))
        givenChainReadyFor(vouchers)
        coEvery { extrinsicService.buildExtrinsic(any(), any(), any(), any()) } returns
            Result.failure(IllegalStateException("no runtime"))

        val result = useCase.initiateUnload(vouchers, destination, NO_SURPLUS, groupId)

        assertTrue(result.isFailure)
        coVerify(exactly = 0) { transactionService.submitTransactions(any(), any()) }
    }

    @Test
    fun `recycler revisions that cannot be read fail the unload`() = runBlocking<Unit> {
        val vouchers = listOf(voucherInRecycler(1, recycler = 1))
        givenChainReadyFor(vouchers)
        coEvery { recyclerProofDataProvider.getRecyclerRevisions(any(), any(), any()) } returns
            Result.failure(IllegalStateException("no proofs"))

        val result = useCase.initiateUnload(vouchers, destination, NO_SURPLUS, groupId)

        assertTrue(result.isFailure)
        coVerify(exactly = 0) { transactionService.submitTransactions(any(), any()) }
    }

    // ---- status ----

    /** Registered but not yet decided: the destination has been promised the money and nothing has moved. */
    @Test
    fun `an unload whose transactions are still live reports as submitted`() = runBlocking {
        givenGroupReports(listOf(entry(PENDING_SUCCESS), entry(FINALIZED_SUCCESS)))

        assertEquals(ExternalUnloadStatus.Submitted, statuses().last())
    }

    /**
     * A group the ledger has no rows for yet — registration has not committed. Reporting anything else would
     * describe an unload that has not started as one that failed.
     */
    @Test
    fun `an unload the ledger has no record of yet reports as submitted`() = runBlocking {
        givenGroupReports(emptyList())

        assertEquals(ExternalUnloadStatus.Submitted, statuses().last())
    }

    @Test
    fun `an unload whose every transaction finalized reports success`() = runBlocking {
        givenGroupReports(listOf(entry(FINALIZED_SUCCESS), entry(FINALIZED_SUCCESS)))

        assertEquals(ExternalUnloadStatus.FinalizedSuccess, statuses().last())
    }

    /**
     * Two of three groups executed and the third did not. Money did move, so this is not a failure — but the
     * destination got less than it was promised, and the caller has to be able to say by how much.
     *
     * One executed group unloaded 8 and folded 2 back into a fresh voucher, the other unloaded 4, and the
     * failed one would have unloaded 16: the destination got 8 - 2 + 4.
     */
    @Test
    fun `an unload where some transactions executed reports how many and what reached the destination`() = runBlocking {
        val withSurplus = voucherInRecycler(1, exponent = 3)
        val surplus = voucherOf(2, Location.Onboarding, exponent = 1)
        val plain = voucherInRecycler(3, exponent = 2)
        val neverUnloaded = voucherInRecycler(4, exponent = 4)
        givenVouchersKnown(withSurplus, surplus, plain, neverUnloaded)

        givenGroupReports(
            listOf(
                entry(FINALIZED_SUCCESS, unloads = listOf(withSurplus), mints = listOf(surplus)),
                entry(FINALIZED_SUCCESS, unloads = listOf(plain), mints = emptyList()),
                entry(FAILURE, unloads = listOf(neverUnloaded), mints = emptyList()),
            )
        )

        assertEquals(ExternalUnloadStatus.PartialSuccess(executed = 2, total = 3, claimed = planks(10)), statuses().last())
    }

    /**
     * The app died after registering the unload, and by the time the caller retries the vouchers are locked by
     * it, or already spent. The retry must still join the unload instead of refusing vouchers that were valid
     * when it was submitted.
     */
    @Test
    fun `a group an earlier attempt already submitted is joined whatever the vouchers look like now`() = runBlocking<Unit> {
        coEvery { transactionService.getOperationGroupStatuses(groupId) } returns
            Result.success(listOf(entry(PENDING)))

        val result = useCase.initiateUnload(emptyList(), destination, NO_SURPLUS, groupId)

        assertTrue(result.isSuccess)
        coVerify(exactly = 0) { transactionService.submitTransactions(any(), any()) }
    }

    @Test
    fun `an unload where nothing executed reports failure`() = runBlocking {
        givenGroupReports(listOf(entry(FAILURE), entry(FAILURE)))

        assertEquals(ExternalUnloadStatus.Failed, statuses().last())
    }

    /** Nothing in the group can change again, so the caller is not left with a subscription open forever. */
    @Test
    fun `reporting ends once every transaction is decided`() = runBlocking {
        givenGroupReports(listOf(entry(FINALIZED_SUCCESS)))

        assertEquals(1, statuses().size)
    }

    /** A live group keeps reporting, so a caller sees the unload settle rather than only its first state. */
    @Test
    fun `an unload still in flight keeps reporting until it settles`() = runBlocking {
        givenGroupReports(
            listOf(entry(PENDING)),
            listOf(entry(FINALIZED_SUCCESS)),
        )

        assertEquals(
            listOf(ExternalUnloadStatus.Submitted, ExternalUnloadStatus.FinalizedSuccess),
            useCase.subscribeUnloadStatus(groupId).take(2).toList(),
        )
    }

    private val registered = mutableListOf<CoinageTransactionRequest>()

    /**
     * Everything an unload consults before it can build a call: the chain, the active people collection and
     * its unload tokens, a revision per recycler, and one alias per voucher.
     */
    private fun givenChainReadyFor(vouchers: List<RecyclerVoucher>) {
        val chain: Chain = mockk()
        val entropy = BandersnatchEntropy(byteArrayOf(1))
        val recyclerKeys = vouchers.map { RecyclerKey(it.recyclerValue, it.recyclerLocationOrThrow().recyclerIndex) }

        every { chain.id } returns CHAIN_ID
        coEvery { chainAssetProvider.chainId() } returns CHAIN_ID
        coEvery { chainRegistry.getChain(CHAIN_ID) } returns chain
        coEvery { transactionService.getOperationGroupStatuses(groupId) } returns Result.success(emptyList())
        coEvery { coinageBalanceConverterUseCase.create() } returns Result.success(mockk())
        coEvery { activePeopleCollectionUseCase.getActivePeopleCollection() } returns PeopleCollection.People
        coEvery { rpcCalls.getBlockHash(CHAIN_ID) } returns BLOCK_HASH

        val resolver: FreeUnloadTokenResolver = mockk()
        every { unloadTokenResolverFactory.createForPeople() } returns resolver
        coEvery { resolver.resolve(CHAIN_ID, any()) } answers {
            List(secondArg<Int>()) { FreeUnloadTokenResolver.ResolvedUnloadToken(0L, it.toLong(), CONTEXT) }
        }

        coEvery { recyclerProofDataProvider.getRecyclerRevisions(any(), any(), any()) } returns
            Result.success(recyclerKeys.distinct().associateWith { RingRevision(1) })

        every { coinageSigningContextProvider.recyclerVouchersContext() } returns CONTEXT
        coEvery { voucherRingDerivation.deriveBandersnatch(any()) } returns entropy
        every { entropy.aliasInContext(any()) } returns mockk(relaxed = true)

        coEvery { peopleMembershipProver.precomputeForMember(any(), any(), any()) } returns Result.success(mockk())

        coEvery { extrinsicService.buildExtrinsic(any(), any(), any(), any()) } returns Result.success(mockk())
        coEvery { transactionService.submitTransactions(any(), groupId) } answers {
            registered += firstArg<List<CoinageTransactionRequest>>()

            Result.success(registered.map { CoinageTransactionId(1) })
        }
    }

    private fun givenNothingSubmittedYet() {
        coEvery { transactionService.getOperationGroupStatuses(groupId) } returns Result.success(emptyList())
    }

    private suspend fun statuses() = useCase.subscribeUnloadStatus(groupId).toList()

    private fun givenGroupReports(vararg emissions: List<CoinageTransactionState>) {
        coEvery { coinageBalanceConverterUseCase.create() } returns Result.success(PowerOfTwoConversion)
        every { transactionService.subscribeOperationGroupStatuses(groupId) } returns flowOf(*emissions)
    }

    private fun givenVouchersKnown(vararg vouchers: RecyclerVoucher) {
        coEvery { voucherRepository.getByRingVrfKeyIndices(any()) } answers {
            val keys = firstArg<List<CoinageKeyIndex>>()
            vouchers.filter { it.ringVrfKeyIndex in keys }
        }
    }

    private fun entry(status: DurableTxStatus, unloads: List<RecyclerVoucher>, mints: List<RecyclerVoucher>) =
        CoinageTransactionState(
            id = CoinageTransactionId(status.ordinal.toLong()),
            status = status,
            inputs = unloads.map { CoinageInput.Voucher(it.ringVrfKeyIndex) },
            outputs = mints.map { OwnAsset.Voucher(it.ringVrfKeyIndex) },
        )

    private fun entry(status: DurableTxStatus) = CoinageTransactionState(
        id = CoinageTransactionId(status.ordinal.toLong()),
        status = status,
        inputs = listOf(CoinageInput.Voucher(testKey(status.ordinal))),
        outputs = listOf(OwnAsset.Voucher(testKey(status.ordinal))),
    )

    private fun voucherInRecycler(index: Int, recycler: Int = index, exponent: Int = 1) = voucherOf(
        index = index,
        location = Location.InRecycler(RecyclerIndex(BigInteger.valueOf(recycler.toLong())), recyclerMembers = 767, enteredAt = null),
        exponent = exponent,
    )

    private fun voucherOf(index: Int, location: Location, exponent: Int = 1) = RecyclerVoucher(
        ringVrfKeyIndex = testKey(index),
        ringVrfPublicKey = byteArrayOf(index.toByte()).toDataByteArray(),
        recyclerValue = ValueExponent(exponent),
        location = location,
        recyclerFungibility = RecyclerFungibility.NONE,
        maxRecyclerFungibility = RecyclerFungibility.NONE,
    )

    private companion object {
        val NO_SURPLUS = BigInteger.ZERO.intoBalance()
        val CONTEXT = BandersnatchContext(byteArrayOf(9))

        const val CHAIN_ID = "test-chain"
        const val BLOCK_HASH = "0xrevision"
        const val BANDERSNATCH_FILE = "io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropyKt"
    }
}
