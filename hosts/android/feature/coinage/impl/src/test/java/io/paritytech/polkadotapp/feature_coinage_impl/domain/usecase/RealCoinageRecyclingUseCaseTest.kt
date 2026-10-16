package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropy
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.VoucherAllocator
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetStates
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins.CoinageTransactionOrigins
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.FormMultiExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.StoringMultiExtrinsicBuilder
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FAILURE
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING_SUCCESS
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.math.BigInteger

class RealCoinageRecyclingUseCaseTest {
    private val coinRepository: CoinRepository = mockk()
    private val voucherAllocator: VoucherAllocator = mockk()
    private val voucherRingDerivation: VoucherRingDerivation = mockk(relaxed = true)
    private val transactionService: CoinageTransactionService = mockk()
    private val coinageTransactionOrigins: CoinageTransactionOrigins = mockk(relaxed = true)
    private val chainConnectionRefCounter: ChainConnectionRefCounter = mockk()
    private val chainRegistry: ChainRegistry = mockk()
    private val extrinsicService: ExtrinsicService = mockk()
    private val chainAssetProvider: ChainAssetProvider = mockk()
    private val voucherRepository: VoucherRepository = mockk()

    private val recyclingAge = 14
    private val chainId = "test-chain-id"

    init {

        coEvery { coinRepository.getCoinRecyclingAge() } returns Result.success(recyclingAge)
        every { chainAssetProvider.chainId() } returns chainId
        coEvery { chainConnectionRefCounter.requestConnectionEnabled(any(), any()) } returns mockk(relaxed = true)
        coEvery { chainRegistry.getChain(any()) } returns mockk(relaxed = true)
        coEvery { transactionService.getOperationGroupStatuses(any()) } returns Result.success(emptyList())
        coEvery { transactionService.getAssetStates(any()) } returns Result.success(CoinageAssetStates(emptyMap(), TEST_INSTALLATION))
    }

    /**
     * Built per test, because it moves its work onto [CoroutineDispatchers.computation] and that has to be
     * this test's scheduler — anywhere else and the work would run off the test's clock.
     */
    private fun TestScope.createUseCase() = RealCoinageRecyclingUseCase(
        coinRepository = coinRepository,
        voucherAllocator = voucherAllocator,
        voucherRingDerivation = voucherRingDerivation,
        coinageTransactionOrigins = coinageTransactionOrigins,
        chainConnectionRefCounter = chainConnectionRefCounter,
        chainRegistry = chainRegistry,
        extrinsicService = extrinsicService,
        chainAssetProvider = chainAssetProvider,
        transactionService = transactionService,
        voucherRepository = voucherRepository,
        dispatchers = testDispatchers(),
    )

    @Test
    fun `returns success when no coins to recycle`() = runTest {
        val useCase = createUseCase()

        val result = useCase.recycle(emptyList())

        assertTrue(result.isSuccess)
    }

    @Test
    fun `every coin to recycle is registered, in one batch under one group`() = runTest {
        val useCase = createUseCase()

        val coins = listOf(createCoin(exponent = 1), createCoin(exponent = 2))
        val vouchers = listOf(createVoucher(ringVrfKeyIndex = 5, exponent = 1), createVoucher(ringVrfKeyIndex = 6, exponent = 2))
        withAllocatedVouchers(coins, vouchers)
        withBuiltExtrinsics(count = 2)

        val result = useCase.recycle(coins)

        assertTrue("recycle failed: ${result.exceptionOrNull()}", result.isSuccess)
        verifyRegisteredUnderOneGroup(coins, vouchers)
    }

    @Test
    fun `every extrinsic is built before any is registered so their nonces stay sequenced`() = runTest {
        val useCase = createUseCase()

        val coins = listOf(createCoin(exponent = 1), createCoin(exponent = 2))
        val vouchers = listOf(createVoucher(ringVrfKeyIndex = 5, exponent = 1), createVoucher(ringVrfKeyIndex = 6, exponent = 2))
        withAllocatedVouchers(coins, vouchers)
        withBuiltExtrinsics(count = 2)

        useCase.recycle(coins)

        coVerifyOrder {
            extrinsicService.buildExtrinsics(any(), any(), any())
            transactionService.submitTransactions(any(), any())
        }
    }

    @Test
    fun `returns failure when building the batch fails`() = runTest {
        val useCase = createUseCase()

        val coins = listOf(createCoin(exponent = 1))
        withAllocatedVouchers(coins, listOf(createVoucher(ringVrfKeyIndex = 5, exponent = 1)))
        coEvery { extrinsicService.buildExtrinsics(any(), any(), any()) } returns
            Result.failure(RuntimeException("Batch failed"))

        val result = useCase.recycle(coins)

        assertTrue(result.isFailure)
    }

    /**
     * Recycling is triggered from a worker, from a payment that needs vouchers, and from a button the user
     * can press twice. Two runs that overlap read the same due coins, and the second would allocate a
     * voucher per coin before the ledger refused it for reusing inputs — spending voucher indices on
     * transactions that cannot be registered.
     */
    @Test
    fun `a coin the ledger already has in flight is not recycled again`() = runTest {
        val useCase = createUseCase()

        val inFlight = createCoin(exponent = 1)
        val idle = createCoin(exponent = 2)
        val voucher = createVoucher(ringVrfKeyIndex = 6, exponent = 2)

        withLedgerHolding(inFlight)
        withAllocatedVouchers(listOf(idle), listOf(voucher))
        withBuiltExtrinsics(count = 1)

        val result = useCase.recycle(listOf(inFlight, idle))

        assertTrue("recycle failed: ${result.exceptionOrNull()}", result.isSuccess)
        verifyRegisteredUnderOneGroup(listOf(idle), listOf(voucher))
        coVerify(exactly = 1) { voucherAllocator.allocate(any()) }
    }

    /** Nothing to do is not a failure, and nothing may be spent finding that out. */
    @Test
    fun `nothing is submitted when every due coin is already in flight`() = runTest {
        val useCase = createUseCase()

        val coins = listOf(createCoin(exponent = 1), createCoin(exponent = 2))

        withLedgerHolding(*coins.toTypedArray())

        val result = useCase.recycle(coins)

        assertTrue("recycle failed: ${result.exceptionOrNull()}", result.isSuccess)
        coVerify(exactly = 0) { voucherAllocator.allocate(any()) }
        coVerify(exactly = 0) { transactionService.submitTransactions(any(), any()) }
    }

    /** A coin whose key has left the device is not ours to recycle either. */
    @Test
    fun `a coin handed off to a peer is not recycled`() = runTest {
        val useCase = createUseCase()

        val handedOff = createCoin(exponent = 1)

        coEvery { transactionService.getAssetStates(any()) } returns Result.success(
            CoinageAssetStates(
                mapOf(
                    OwnAsset.Coin(handedOff.derivationIndex) to
                        CoinageAssetState(handedOff = true, minterStatus = FINALIZED_SUCCESS, consumerStatus = null)
                ),
                TEST_INSTALLATION,
            )
        )

        val result = useCase.recycle(listOf(handedOff))

        assertTrue(result.isSuccess)
        coVerify(exactly = 0) { transactionService.submitTransactions(any(), any()) }
    }

    /** The ledger says nothing about a coin no transaction of ours has touched, and that means it is free. */
    private fun withLedgerHolding(vararg coins: Coin) {
        val held: Map<OwnAsset, CoinageAssetState> = coins.associate {
            OwnAsset.Coin(it.derivationIndex) to
                CoinageAssetState(handedOff = false, minterStatus = FINALIZED_SUCCESS, consumerStatus = PENDING)
        }

        coEvery { transactionService.getAssetStates(any()) } returns Result.success(CoinageAssetStates(held, TEST_INSTALLATION))
    }

    /**
     * A voucher has to be allocated for each coin before it can be loaded into a recycler. One that cannot
     * be allocated costs that coin its turn and nothing else — the coins that did get one are still worth
     * recycling, and this one comes back round next cycle.
     */
    @Test
    fun `a coin whose voucher cannot be allocated is left for next time`() = runTest {
        val useCase = createUseCase()

        val allocatable = createCoin(exponent = 1)
        val unallocatable = createCoin(exponent = 2)
        val voucher = createVoucher(ringVrfKeyIndex = 5, exponent = 1)

        withAllocatedVouchers(listOf(allocatable), listOf(voucher))
        withBuiltExtrinsics(count = 1)

        val result = useCase.recycle(listOf(allocatable, unallocatable))

        assertTrue("recycle failed: ${result.exceptionOrNull()}", result.isSuccess)
        verifyRegisteredUnderOneGroup(listOf(allocatable), listOf(voucher))
    }

    @Test
    fun `nothing is submitted when no voucher could be allocated`() = runTest {
        val useCase = createUseCase()

        withAllocatedVouchers(emptyList(), emptyList())

        val result = useCase.recycle(listOf(createCoin(exponent = 1)))

        assertTrue("recycle failed: ${result.exceptionOrNull()}", result.isSuccess)
        coVerify(exactly = 0) { transactionService.submitTransactions(any(), any()) }
    }

    /**
     * The ledger could not be read, so which coins are already spoken for is unknown. Recycling one of those
     * is the collision this avoids, so nothing is touched — not even a voucher allocated.
     */
    @Test
    fun `nothing is recycled when the ledger cannot be read`() = runTest {
        val useCase = createUseCase()

        coEvery { transactionService.getAssetStates(any()) } returns
            Result.failure(IllegalStateException("no ledger"))

        val result = useCase.recycle(listOf(createCoin(exponent = 1)))

        assertTrue("recycle failed: ${result.exceptionOrNull()}", result.isSuccess)
        coVerify(exactly = 0) { voucherAllocator.allocate(any()) }
        coVerify(exactly = 0) { transactionService.submitTransactions(any(), any()) }
    }

    /**
     * Each coin is loaded into the recycler under an origin of its own — the coin is what authorises the
     * call, so one built under another coin's origin would be spending the wrong coin.
     */
    @Test
    fun `each coin is loaded into the recycler under its own origin`() = runTest {
        val useCase = createUseCase()

        val coins = listOf(createCoin(exponent = 1), createCoin(exponent = 2))
        val vouchers = listOf(createVoucher(ringVrfKeyIndex = 5, exponent = 1), createVoucher(ringVrfKeyIndex = 6, exponent = 2))

        withAllocatedVouchers(coins, vouchers)
        withFormedExtrinsics(count = coins.size)

        val result = useCase.recycle(coins)
        assertTrue("recycle failed: ${result.exceptionOrNull()}", result.isSuccess)

        coVerify { coinageTransactionOrigins.createAsCoinOrigin(coins[0]) }
        coVerify { coinageTransactionOrigins.createAsCoinOrigin(coins[1]) }
        coVerify { voucherRingDerivation.deriveBandersnatch(vouchers[0].ringVrfKeyIndex) }
        coVerify { voucherRingDerivation.deriveBandersnatch(vouchers[1].ringVrfKeyIndex) }
    }

    // ---- caller's group ----

    @Test
    fun `recycling under a caller's group registers under that group`() = runTest {
        val useCase = createUseCase()

        val coins = listOf(createCoin(exponent = 1))
        withAllocatedVouchers(coins, listOf(createVoucher(ringVrfKeyIndex = 5, exponent = 1)))
        withBuiltExtrinsics(count = 1)

        useCase.recycle(coins, GROUP)

        assertEquals(GROUP, batches.single().groupId)
    }

    /** The payment that asked for this recycling died and is retrying it: those coins are already on their way. */
    @Test
    fun `a group an earlier attempt already submitted is not recycled again`() = runTest {
        val useCase = createUseCase()
        coEvery { transactionService.getOperationGroupStatuses(GROUP) } returns
            Result.success(listOf(recyclingInto(5, PENDING)))

        val result = useCase.recycle(listOf(createCoin(exponent = 1)), GROUP)

        assertTrue(result.isSuccess)
        coVerify(exactly = 0) { transactionService.submitTransactions(any(), any()) }
    }

    @Test
    fun `a ledger that cannot be read fails recycling under a caller's group`() = runTest {
        val useCase = createUseCase()
        coEvery { transactionService.getOperationGroupStatuses(GROUP) } returns
            Result.failure(IllegalStateException("no ledger"))

        val result = useCase.recycle(listOf(createCoin(exponent = 1)), GROUP)

        assertTrue(result.isFailure)
        coVerify(exactly = 0) { transactionService.submitTransactions(any(), any()) }
    }

    // ---- status ----

    @Test
    fun `recycling still in flight is pending`() = runTest {
        givenRecycling(listOf(recyclingInto(5, PENDING_SUCCESS), recyclingInto(6, PENDING)), vouchers = emptyList())

        assertEquals(RecyclingStatus.Pending, createUseCase().observeRecyclingStatus(GROUP).first())
    }

    @Test
    fun `a coin that will never become a voucher leaves the recycling incomplete`() = runTest {
        givenRecycling(listOf(recyclingInto(5, FINALIZED_SUCCESS), recyclingInto(6, FAILURE)), vouchers = emptyList())

        assertEquals(RecyclingStatus.Incomplete, createUseCase().observeRecyclingStatus(GROUP).first())
    }

    /** Nothing was registered, so nothing will ever arrive — waiting on it would wait forever. */
    @Test
    fun `a group with nothing registered is incomplete`() = runTest {
        givenRecycling(emptyList(), vouchers = emptyList())

        assertEquals(RecyclingStatus.Incomplete, createUseCase().observeRecyclingStatus(GROUP).first())
    }

    /** The unload that follows needs the recycler the voucher sits in, which the location sync reports later. */
    @Test
    fun `recycled at the best head but not yet seen in its recycler is still pending`() = runTest {
        givenRecycling(
            listOf(recyclingInto(5, PENDING_SUCCESS)),
            vouchers = listOf(createVoucher(ringVrfKeyIndex = 5, exponent = 1, location = RecyclerVoucher.Location.Onboarding)),
        )

        assertEquals(RecyclingStatus.Pending, createUseCase().observeRecyclingStatus(GROUP).first())
    }

    @Test
    fun `recycled at the best head reports the vouchers it became, not yet finalized`() = runTest {
        val vouchers = listOf(voucherInRecycler(5), voucherInRecycler(6))
        givenRecycling(listOf(recyclingInto(5, PENDING_SUCCESS), recyclingInto(6, FINALIZED_SUCCESS)), vouchers)

        assertEquals(
            RecyclingStatus.AllRecycled(vouchers = vouchers, finalized = false),
            createUseCase().observeRecyclingStatus(GROUP).first(),
        )
    }

    @Test
    fun `recycling finalized for every coin reports finalized`() = runTest {
        val vouchers = listOf(voucherInRecycler(5), voucherInRecycler(6))
        givenRecycling(listOf(recyclingInto(5, FINALIZED_SUCCESS), recyclingInto(6, FINALIZED_SUCCESS)), vouchers)

        assertEquals(
            RecyclingStatus.AllRecycled(vouchers = vouchers, finalized = true),
            createUseCase().observeRecyclingStatus(GROUP).first(),
        )
    }

    private fun givenRecycling(states: List<CoinageTransactionState>, vouchers: List<RecyclerVoucher>) {
        every { transactionService.subscribeOperationGroupStatuses(GROUP) } returns flowOf(states)
        every { voucherRepository.subscribeAllVouchers() } returns flowOf(vouchers)
    }

    private fun recyclingInto(voucherIndex: Int, status: DurableTxStatus) = CoinageTransactionState(
        id = CoinageTransactionId(voucherIndex.toLong()),
        status = status,
        inputs = listOf(CoinageInput.Coin.Own(testKey(100 + voucherIndex))),
        outputs = listOf(OwnAsset.Voucher(testKey(voucherIndex))),
    )

    private fun voucherInRecycler(ringVrfKeyIndex: Int) = createVoucher(
        ringVrfKeyIndex = ringVrfKeyIndex,
        exponent = 1,
        location = RecyclerVoucher.Location.InRecycler(RecyclerIndex(BigInteger.ONE), recyclerMembers = 32, enteredAt = null),
    )

    /** Runs the block the use case hands to the builder, instead of only checking that it was handed over. */
    private fun withFormedExtrinsics(count: Int) {
        // BandersnatchEntropy is a value class over ByteArray, which a relaxed mock cannot produce — it
        // hands back a bare Object and the cast fails inside the code under test.
        coEvery { voucherRingDerivation.deriveBandersnatch(any()) } returns BandersnatchEntropy(byteArrayOf(1))
        withBuiltExtrinsics(count)

        coEvery { extrinsicService.buildExtrinsics(any(), any(), any()) } coAnswers {
            thirdArg<FormMultiExtrinsic>().invoke(StoringMultiExtrinsicBuilder())

            Result.success(List(count) { mockk<EnrichedSendableExtrinsic>() })
        }
    }

    private fun createCoin(exponent: Int): Coin {
        return Coin(
            derivationIndex = testKey(exponent),
            valueExponent = ValueExponent(exponent),
            age = Coin.Age.Known(recyclingAge),
            isOnChain = true,
            accountId = byteArrayOf(exponent.toByte()).intoAccountId(),
            provenance = CoinProvenance.UNKNOWN,
        )
    }

    private fun createVoucher(
        ringVrfKeyIndex: Int,
        exponent: Int,
        location: RecyclerVoucher.Location = RecyclerVoucher.Location.Unknown,
    ): RecyclerVoucher {
        return RecyclerVoucher(
            ringVrfKeyIndex = testKey(ringVrfKeyIndex),
            ringVrfPublicKey = byteArrayOf(ringVrfKeyIndex.toByte()).toDataByteArray(),
            recyclerValue = ValueExponent(exponent),
            location = location,
            recyclerFungibility = RecyclerFungibility.NONE,
            maxRecyclerFungibility = RecyclerFungibility.NONE,
        )
    }

    private fun withAllocatedVouchers(coins: List<Coin>, vouchers: List<RecyclerVoucher>) {
        val byExponent = coins.map { it.valueExponent }.zip(vouchers).toMap()

        // An inline-class argument still arrives unboxed inside `answers`, so it is rewrapped here.
        coEvery { voucherAllocator.allocate(any()) } answers {
            val exponent = ValueExponent(firstArg())

            byExponent[exponent]?.let { Result.success(it) }
                ?: Result.failure(IllegalStateException("no voucher for $exponent"))
        }
        coEvery { voucherAllocator.allocateAll(any()) } answers {
            runCatching {
                firstArg<List<ValueExponent>>().map { byExponent[it] ?: error("no voucher for $it") }
            }
        }
    }

    private class Batch(
        val requests: List<CoinageTransactionRequest>,
        val groupId: CoinageOperationGroupId,
    )

    private val batches = mutableListOf<Batch>()

    private fun withBuiltExtrinsics(count: Int) {
        val extrinsics = List(count) { mockk<EnrichedSendableExtrinsic>() }
        coEvery { extrinsicService.buildExtrinsics(any(), any(), any()) } returns Result.success(extrinsics)

        coEvery { transactionService.submitTransactions(any(), any()) } answers {
            batches += Batch(
                requests = arg(0),
                // A value class over String arrives unboxed at the mock boundary.
                groupId = CoinageOperationGroupId(arg(1)),
            )

            Result.success(batches.last().requests.indices.map { CoinageTransactionId(it.toLong()) })
        }
    }

    private fun verifyRegisteredUnderOneGroup(coins: List<Coin>, vouchers: List<RecyclerVoucher>) {
        // One batch, so the shared group is now structural rather than something each call has to agree on.
        val batch = batches.single()

        assertEquals(coins.size, batch.requests.size)
        assertEquals(
            coins.map { listOf(CoinageInput.Coin.Own(it.derivationIndex)) },
            batch.requests.map { it.inputs },
        )
        assertEquals(
            vouchers.map { listOf(OwnAsset.Voucher(it.ringVrfKeyIndex)) },
            batch.requests.map { it.outputs },
        )
    }

    private companion object {
        val GROUP = CoinageOperationGroupId("external-payment-recycle:origin:id")
    }
}
