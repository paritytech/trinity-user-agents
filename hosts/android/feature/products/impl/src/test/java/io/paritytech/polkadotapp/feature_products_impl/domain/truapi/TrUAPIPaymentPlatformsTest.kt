package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TotalBalanceUseCase
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.PaymentRequestError
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.ProductPaymentRequestId
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.RequestPaymentUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.ShortfallDisclosure
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpId
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpSource
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpError
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpService
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpStatus
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.DerivationIndex
import uniffi.truapi.HostPaymentBalanceSubscribeException
import uniffi.truapi.HostPaymentException
import uniffi.truapi.HostPaymentRequest
import uniffi.truapi.HostPaymentStatusSubscribeItem
import uniffi.truapi.HostPaymentTopUpException
import uniffi.truapi.HostPaymentTopUpRequest
import uniffi.truapi.HostPaymentTopUpStatusSubscribeItem
import uniffi.truapi.HostRejection
import uniffi.truapi.PaymentTopUpSource as NativeTopUpSource

/**
 * The core asks for an operation's status once and is told every later one, so a status the engine reports
 * and the adapter does not push is a status the product never sees.
 */
class TrUAPIPaymentPlatformsTest {
    private val topUpService: TopUpService = mockk()
    private val requestPaymentUseCase: RequestPaymentUseCase = mockk()
    private val totalBalanceUseCase: TotalBalanceUseCase = mockk()
    private val appState = MutableStateFlow(AppLifecycleState.FOREGROUND)
    private val appLifecycleObserver = object : AppLifecycleObserver {
        override fun subscribe() = appState

        override fun getCurrentState() = appState.value
    }

    private val platforms = TrUAPIPaymentPlatforms(
        topUpService = topUpService,
        requestPaymentUseCase = requestPaymentUseCase,
        totalBalanceUseCase = totalBalanceUseCase,
        appLifecycleObserver = appLifecycleObserver,
        dispatchers = TestCoroutineDispatchers(Dispatchers.IO),
    )

    private val pushedTopUps = MutableStateFlow<List<HostPaymentTopUpStatusSubscribeItem>>(emptyList())
    private val pushedPayments = MutableStateFlow<List<HostPaymentStatusSubscribeItem>>(emptyList())
    private val pushedBalances = MutableStateFlow<List<Balance>>(emptyList())

    private val product = ProductId.fromStoredValue("product.dot")
    private val rawId = ByteArray(32) { 4 }
    private val topUpId = PaymentTopUpId.fromBytes(rawId.toDataByteArray()).getOrThrow()
    private val paymentId = ProductPaymentRequestId.fromBytes(rawId.toDataByteArray()).getOrThrow()

    init {
        platforms.attach(
            object : TrUAPIPaymentSink {
                override fun topUpStatus(productId: String, id: ByteArray, status: HostPaymentTopUpStatusSubscribeItem) {
                    pushedTopUps.update { it + status }
                }

                override fun paymentStatus(productId: String, id: ByteArray, status: HostPaymentStatusSubscribeItem) {
                    pushedPayments.update { it + status }
                }

                override fun balance(available: Balance) {
                    pushedBalances.update { it + available }
                }
            }
        )
    }

    // ---- top-ups ----

    @Test
    fun `a started top-up is pushed until it is final and no further`() = runBlocking<Unit> {
        val source = PaymentTopUpSource.ProductAccount(DerivationIndex32.fromUInt(3u))
        coEvery { topUpService.start(product, topUpId, planks(25), source) } returns Result.success(Unit)
        every { topUpService.status(product, topUpId) } returns flowOf(
            TopUpStatus.Detecting,
            TopUpStatus.Claiming,
            TopUpStatus.Claimed(finalized = true),
            TopUpStatus.NotClaimed,
        )

        platforms.topUp(product.value, topUpRequest(NativeTopUpSource.ProductAccount(DerivationIndex.Index(3u))))

        assertEquals(
            listOf(
                HostPaymentTopUpStatusSubscribeItem.Detecting,
                HostPaymentTopUpStatusSubscribeItem.Claiming,
                HostPaymentTopUpStatusSubscribeItem.Claimed(finalized = true),
            ),
            pushedTopUps.awaitSize(3),
        )
    }

    /** After a restart nothing is forwarding yet: the core's first ask has to start it, or later statuses are lost. */
    @Test
    fun `asking after a top-up resumes forwarding from its current status`() = runBlocking<Unit> {
        val engine = MutableStateFlow<TopUpStatus>(TopUpStatus.Claiming)
        every { topUpService.status(product, topUpId) } returns engine

        val current = platforms.topUpStatus(product.value, rawId)
        platforms.topUpStatus(product.value, rawId)
        engine.value = TopUpStatus.ClaimedPartially(actualClaimed = planks(7))

        assertEquals(HostPaymentTopUpStatusSubscribeItem.Claiming, current)
        assertEquals(
            listOf(
                HostPaymentTopUpStatusSubscribeItem.Claiming,
                HostPaymentTopUpStatusSubscribeItem.ClaimedPartially(actualClaimed = "7"),
            ),
            pushedTopUps.awaitSize(2),
        )
        verify(exactly = 1) { topUpService.status(product, topUpId) }
    }

    @Test
    fun `a top-up the host never registered has no status`() {
        every { topUpService.status(product, topUpId) } returns flow { throw TopUpError.NotFound(topUpId) }

        assertNull(platforms.topUpStatus(product.value, rawId))
    }

    /** The ask runs on the core's thread, so an engine that has not answered yet must not hold it. */
    @Test
    fun `a top-up the engine has not answered for yet reads as detecting, and its status follows`() = runBlocking<Unit> {
        val engine = MutableSharedFlow<TopUpStatus>()
        every { topUpService.status(product, topUpId) } returns engine

        val started = System.nanoTime()
        val current = platforms.topUpStatus(product.value, rawId)
        val waitedMs = (System.nanoTime() - started) / 1_000_000

        assertEquals(HostPaymentTopUpStatusSubscribeItem.Detecting, current)
        assertTrue("waited ${waitedMs}ms", waitedMs < 3_000)

        engine.emit(TopUpStatus.Claiming)
        assertEquals(listOf(HostPaymentTopUpStatusSubscribeItem.Claiming), pushedTopUps.awaitSize(1))
    }

    @Test
    fun `an engine failure reaches the core with a fixed reason, not the engine's words`() {
        every { topUpService.status(product, topUpId) } returns flow { throw IllegalStateException("engine down at node 7") }

        val error = runCatching { platforms.topUpStatus(product.value, rawId) }.exceptionOrNull()

        assertEquals("status is unavailable", (error as HostRejection.Rejected).reason)
    }

    /** The core was told detecting; once the engine says the top-up is unknown, the next ask must say so. */
    @Test
    fun `a top-up found unknown after the wait answers null on the next ask`() = runBlocking<Unit> {
        val engineAnswers = CompletableDeferred<Unit>()
        every { topUpService.status(product, topUpId) } returns flow {
            engineAnswers.await()
            throw TopUpError.NotFound(topUpId)
        }

        assertEquals(HostPaymentTopUpStatusSubscribeItem.Detecting, platforms.topUpStatus(product.value, rawId))

        engineAnswers.complete(Unit)
        withTimeout(5_000) {
            while (platforms.topUpStatus(product.value, rawId) != null) delay(20)
        }
        assertTrue(pushedTopUps.value.isEmpty())
    }

    @Test
    fun `asking about a settled top-up again answers without reading the engine`() = runBlocking<Unit> {
        every { topUpService.status(product, topUpId) } returns flowOf(TopUpStatus.Claimed(finalized = true))

        val first = platforms.topUpStatus(product.value, rawId)
        pushedTopUps.awaitSize(1)
        val again = platforms.topUpStatus(product.value, rawId)

        assertEquals(HostPaymentTopUpStatusSubscribeItem.Claimed(finalized = true), first)
        assertEquals(first, again)
        verify(exactly = 1) { topUpService.status(product, topUpId) }
    }

    /** Two asks at once share one run, so the engine is read once and each status pushed once. */
    @Test
    fun `asks at the same time share one run`() = runBlocking<Unit> {
        val engine = MutableStateFlow<TopUpStatus>(TopUpStatus.Claiming)
        every { topUpService.status(product, topUpId) } returns engine

        val answers = (1..4).map { async(Dispatchers.IO) { platforms.topUpStatus(product.value, rawId) } }.awaitAll()

        assertEquals(List(4) { HostPaymentTopUpStatusSubscribeItem.Claiming }, answers)
        verify(exactly = 1) { topUpService.status(product, topUpId) }
    }

    @Test
    fun `a source another top-up holds is reported as busy`() = runBlocking<Unit> {
        val key = ByteArray(64) { 1 }
        coEvery { topUpService.start(product, topUpId, planks(25), PaymentTopUpSource.PrivateKey(key.toDataByteArray())) } returns
            Result.failure(TopUpError.SourceBusy(topUpId))

        val error = runCatching { platforms.topUp(product.value, topUpRequest(NativeTopUpSource.PrivateKey(key))) }
            .exceptionOrNull()

        assertTrue(error is HostPaymentTopUpException.SourceBusy)
    }

    @Test
    fun `a top-up of nothing, or of no number, is an invalid amount`() = runBlocking<Unit> {
        val key = ByteArray(64) { 1 }

        for (amount in listOf("0", "-5", "ten")) {
            val request = topUpRequest(NativeTopUpSource.PrivateKey(key)).copy(amount = amount)
            val error = runCatching { platforms.topUp(product.value, request) }.exceptionOrNull()

            assertTrue("$amount: $error", error is HostPaymentTopUpException.InvalidAmount)
        }
    }

    @Test
    fun `an unexpected top-up failure reaches the product without the engine's words`() = runBlocking<Unit> {
        val key = ByteArray(64) { 1 }
        coEvery { topUpService.start(product, topUpId, planks(25), any()) } returns
            Result.failure(IllegalStateException("coin 0x12 locked by claim 7"))

        val error = runCatching { platforms.topUp(product.value, topUpRequest(NativeTopUpSource.PrivateKey(key))) }
            .exceptionOrNull()

        assertTrue(error is HostPaymentTopUpException.Unknown)
        assertEquals("top-up could not be registered", (error as HostPaymentTopUpException.Unknown).reason)
    }

    // ---- payments ----

    /** The core hides a shortfall from products without BalanceAccess; the host must always report it. */
    @Test
    fun `a payment the balance cannot cover is reported as insufficient`() = runBlocking<Unit> {
        coEvery {
            requestPaymentUseCase.requestPayment(product, paymentId, planks(25), any(), ShortfallDisclosure.MASKED_BY_CORE)
        } returns Result.failure(PaymentRequestError.InsufficientBalance())

        val error = runCatching { platforms.requestPayment(product.value, paymentRequest()) }.exceptionOrNull()

        assertTrue(error is HostPaymentException.InsufficientBalance)
    }

    @Test
    fun `a payment the host never registered has no status`() {
        every { requestPaymentUseCase.subscribeStatus(product, paymentId) } returns
            flow { throw PaymentRequestError.NotFound(paymentId) }

        assertNull(platforms.paymentStatus(product.value, rawId))
    }

    @Test
    fun `an approved payment is pushed until it settles`() = runBlocking<Unit> {
        coEvery { requestPaymentUseCase.requestPayment(product, paymentId, planks(25), any(), any()) } returns
            Result.success(Unit)
        every { requestPaymentUseCase.subscribeStatus(product, paymentId) } returns
            flowOf(PaymentStatus.Processing, PaymentStatus.PartiallyClaimed(planks(9)))

        platforms.requestPayment(product.value, paymentRequest())

        assertEquals(
            listOf(
                HostPaymentStatusSubscribeItem.Processing,
                HostPaymentStatusSubscribeItem.ClaimedPartially(actualClaimed = "9"),
            ),
            pushedPayments.awaitSize(2),
        )
    }

    @Test
    fun `a payment of nothing, or of no number, is an invalid amount`() = runBlocking<Unit> {
        for (amount in listOf("0", "-5", "ten")) {
            val error = runCatching { platforms.requestPayment(product.value, paymentRequest().copy(amount = amount)) }
                .exceptionOrNull()

            assertTrue("$amount: $error", error is HostPaymentException.InvalidAmount)
        }
    }

    @Test
    fun `an unexpected payment failure reaches the product without the engine's words`() = runBlocking<Unit> {
        coEvery { requestPaymentUseCase.requestPayment(product, paymentId, planks(25), any(), any()) } returns
            Result.failure(IllegalStateException("cheque store at /data/coinage is corrupt"))

        val error = runCatching { platforms.requestPayment(product.value, paymentRequest()) }.exceptionOrNull()

        assertTrue(error is HostPaymentException.Unknown)
        assertEquals("payment could not be started", (error as HostPaymentException.Unknown).reason)
    }

    /** The engine's reason can name a shortfall the core masks, so a failed payment reaches the product generic. */
    @Test
    fun `a failed payment reaches the product with a fixed reason`() = runBlocking<Unit> {
        coEvery { requestPaymentUseCase.requestPayment(product, paymentId, planks(25), any(), any()) } returns
            Result.success(Unit)
        every { requestPaymentUseCase.subscribeStatus(product, paymentId) } returns
            flowOf(PaymentStatus.Processing, PaymentStatus.Failed("only 12 planks spendable"))

        platforms.requestPayment(product.value, paymentRequest())

        assertEquals(
            listOf(
                HostPaymentStatusSubscribeItem.Processing,
                HostPaymentStatusSubscribeItem.Failed(reason = "payment did not complete"),
            ),
            pushedPayments.awaitSize(2),
        )
    }

    // ---- balance ----

    @Test
    fun `the balance is what a payment can spend, and each change is pushed`() = runBlocking<Unit> {
        coEvery { totalBalanceUseCase.getBalance() } returns Result.success(balance(availablePrivate = 10, gainingPrivacy = 5))
        every { totalBalanceUseCase.subscribeTotalBalance() } returns flowOf(
            Result.success(balance(availablePrivate = 10, gainingPrivacy = 5)),
            Result.success(balance(availablePrivate = 12, gainingPrivacy = 5)),
        )

        val available = platforms.balance(product.value, purse = null)

        assertEquals("15", available)
        assertEquals(listOf(planks(15), planks(17)), pushedBalances.awaitSize(2))
    }

    /** Nothing tells the host a product stopped following, so the balance follows the app being on screen. */
    @Test
    fun `the balance is pushed in the foreground and pushed again on coming back`() = runBlocking<Unit> {
        val balances = MutableStateFlow(Result.success(balance(availablePrivate = 10, gainingPrivacy = 5)))
        coEvery { totalBalanceUseCase.getBalance() } returns Result.success(balance(availablePrivate = 10, gainingPrivacy = 5))
        every { totalBalanceUseCase.subscribeTotalBalance() } returns balances

        platforms.balance(product.value, purse = null)
        pushedBalances.awaitSize(1)

        appState.value = AppLifecycleState.BACKGROUND
        balances.value = Result.success(balance(availablePrivate = 11, gainingPrivacy = 5))
        appState.value = AppLifecycleState.FOREGROUND

        assertEquals(listOf(planks(15), planks(16)), pushedBalances.awaitSize(2))
    }

    @Test
    fun `a purse the host does not have is refused`() = runBlocking<Unit> {
        val error = runCatching { platforms.balance(product.value, purse = 1u) }.exceptionOrNull()

        assertTrue(error is HostPaymentBalanceSubscribeException.Unknown)
        coVerify(exactly = 0) { totalBalanceUseCase.getBalance() }
    }

    private suspend fun <T> MutableStateFlow<List<T>>.awaitSize(size: Int): List<T> =
        withTimeout(1_000) { first { it.size >= size } }

    private fun topUpRequest(source: NativeTopUpSource) =
        HostPaymentTopUpRequest(into = null, amount = "25", source = source, id = rawId)

    private fun paymentRequest() =
        HostPaymentRequest(from = null, amount = "25", destination = ByteArray(32) { 7 }, id = rawId)

    private fun balance(availablePrivate: Int, gainingPrivacy: Int) = CoinageBalance(
        availablePrivate = planks(availablePrivate),
        gainingPrivacy = CoinageBalance.GainingPrivacyBalance(amount = planks(gainingPrivacy), canSpendWithConfirmation = false),
        pending = planks(100),
    )

    private fun planks(value: Int): Balance = value.toBigInteger().intoBalance()
}
