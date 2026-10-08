package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
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
import kotlinx.coroutines.Dispatchers
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
import uniffi.truapi.PaymentTopUpSource as NativeTopUpSource

/**
 * The core asks for an operation's status once and is told every later one, so a status the engine reports
 * and the adapter does not push is a status the product never sees.
 */
class TrUAPIPaymentPlatformsTest {
    private val topUpService: TopUpService = mockk()
    private val requestPaymentUseCase: RequestPaymentUseCase = mockk()
    private val totalBalanceUseCase: TotalBalanceUseCase = mockk()

    private val platforms = TrUAPIPaymentPlatforms(
        topUpService = topUpService,
        requestPaymentUseCase = requestPaymentUseCase,
        totalBalanceUseCase = totalBalanceUseCase,
        dispatchers = TestCoroutineDispatchers(Dispatchers.Unconfined),
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

    @Test
    fun `a source another top-up holds is reported as busy`() = runBlocking<Unit> {
        val key = ByteArray(64) { 1 }
        coEvery { topUpService.start(product, topUpId, planks(25), PaymentTopUpSource.PrivateKey(key.toDataByteArray())) } returns
            Result.failure(TopUpError.SourceBusy(topUpId))

        val error = runCatching { platforms.topUp(product.value, topUpRequest(NativeTopUpSource.PrivateKey(key))) }
            .exceptionOrNull()

        assertTrue(error is HostPaymentTopUpException.SourceBusy)
    }

    // ---- payments ----

    /** The core hides a shortfall from products without BalanceAccess; the host must always report it. */
    @Test
    fun `a payment the balance cannot cover is reported as insufficient`() = runBlocking<Unit> {
        coEvery {
            requestPaymentUseCase.requestPayment(product, paymentId, planks(25), any(), ShortfallDisclosure.CALLER)
        } returns Result.failure(PaymentRequestError.InsufficientBalance())

        val error = runCatching { platforms.requestPayment(product.value, paymentRequest()) }.exceptionOrNull()

        assertTrue(error is HostPaymentException.InsufficientBalance)
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
                HostPaymentStatusSubscribeItem.PartiallyClaimed(actualClaimed = "9"),
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
