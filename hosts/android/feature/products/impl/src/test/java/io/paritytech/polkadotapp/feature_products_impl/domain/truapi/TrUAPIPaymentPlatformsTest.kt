package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TotalBalanceUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.PaymentRequestError
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.ProductPaymentRequestId
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.RequestPaymentUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.ShortfallDisclosure
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpId
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpSource
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpError
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpService
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpStatus
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.DerivationIndex
import uniffi.truapi.HostPaymentException
import uniffi.truapi.HostPaymentRequest
import uniffi.truapi.HostPaymentStatusSubscribeException
import uniffi.truapi.HostPaymentStatusSubscribeItem
import uniffi.truapi.HostPaymentTopUpException
import uniffi.truapi.HostPaymentTopUpRequest
import uniffi.truapi.HostPaymentTopUpStatusSubscribeException
import uniffi.truapi.HostPaymentTopUpStatusSubscribeItem
import uniffi.truapi.HostRejection
import uniffi.truapi.ProductId
import uniffi.truapi.PaymentTopUpSource as NativeTopUpSource

/**
 * The core follows each operation and the balance through the adapter, so a status the engine reports and
 * the adapter does not push is a status the product never sees.
 */
class TrUAPIPaymentPlatformsTest {
    private val topUpService: TopUpService = mockk()
    private val requestPaymentUseCase: RequestPaymentUseCase = mockk()
    private val totalBalanceUseCase: TotalBalanceUseCase = mockk()

    private val platforms = TrUAPIPaymentPlatforms(
        topUpService = topUpService,
        requestPaymentUseCase = requestPaymentUseCase,
        totalBalanceUseCase = totalBalanceUseCase,
    )

    private val pushedTopUps = mutableListOf<HostPaymentTopUpStatusSubscribeItem>()
    private val pushedPayments = mutableListOf<HostPaymentStatusSubscribeItem>()
    private val pushedBalances = mutableListOf<Balance>()

    private val product = ProductId.parse("product.dot").getOrThrow()
    private val rawId = ByteArray(32) { 4 }
    private val topUpId = PaymentTopUpId.fromBytes(rawId.toDataByteArray()).getOrThrow()
    private val paymentId = ProductPaymentRequestId.fromBytes(rawId.toDataByteArray()).getOrThrow()

    init {
        platforms.attach(
            object : TrUAPIPaymentSink {
                override fun topUpStatus(productId: ProductId, id: ByteArray, status: HostPaymentTopUpStatusSubscribeItem) {
                    pushedTopUps += status
                }

                override fun paymentStatus(productId: ProductId, id: ByteArray, status: HostPaymentStatusSubscribeItem) {
                    pushedPayments += status
                }

                override fun balance(available: Balance) {
                    pushedBalances += available
                }
            }
        )
    }

    // ---- top-ups ----

    @Test
    fun `a started top-up is accepted without following it`() = runBlocking<Unit> {
        val source = PaymentTopUpSource.ProductAccount(DerivationIndex32.fromUInt(3u))
        coEvery { topUpService.start(product, topUpId, planks(25), source) } returns Result.success(Unit)

        platforms.topUp(product, topUpRequest(NativeTopUpSource.ProductAccount(DerivationIndex.Index(3u))))

        assertEquals(emptyList<HostPaymentTopUpStatusSubscribeItem>(), pushedTopUps)
    }

    @Test
    fun `a followed top-up is pushed until it is final and no further`() = runBlocking<Unit> {
        every { topUpService.status(product, topUpId) } returns flowOf(
            TopUpStatus.Detecting,
            TopUpStatus.Claiming,
            TopUpStatus.Claimed(finalized = true),
            TopUpStatus.NotClaimed,
        )

        platforms.followTopUp(product, rawId)

        assertEquals(
            listOf(
                HostPaymentTopUpStatusSubscribeItem.Detecting,
                HostPaymentTopUpStatusSubscribeItem.Claiming,
                HostPaymentTopUpStatusSubscribeItem.Claimed(finalized = true),
            ),
            pushedTopUps,
        )
    }

    @Test
    fun `following a top-up the host never registered is not found`() = runBlocking<Unit> {
        every { topUpService.status(product, topUpId) } returns flow { throw TopUpError.NotFound(topUpId) }

        val error = runCatching { platforms.followTopUp(product, rawId) }.exceptionOrNull()

        assertTrue("$error", error is HostPaymentTopUpStatusSubscribeException.TopUpNotFound)
    }

    @Test
    fun `an engine failure reaches the core with a fixed reason, not the engine's words`() = runBlocking<Unit> {
        every { topUpService.status(product, topUpId) } returns flow { throw IllegalStateException("engine down at node 7") }

        val error = runCatching { platforms.followTopUp(product, rawId) }.exceptionOrNull()

        assertEquals("status is unavailable", (error as HostPaymentTopUpStatusSubscribeException.Unknown).reason)
    }

    @Test
    fun `a source another top-up holds is reported as busy`() = runBlocking<Unit> {
        val key = ByteArray(64) { 1 }
        coEvery { topUpService.start(product, topUpId, planks(25), PaymentTopUpSource.PrivateKey(key.toDataByteArray())) } returns
            Result.failure(TopUpError.SourceBusy(topUpId))

        val error = runCatching { platforms.topUp(product, topUpRequest(NativeTopUpSource.PrivateKey(key))) }
            .exceptionOrNull()

        assertTrue(error is HostPaymentTopUpException.SourceBusy)
    }

    @Test
    fun `a top-up of nothing, or of no number, is an invalid amount`() = runBlocking<Unit> {
        val key = ByteArray(64) { 1 }

        for (amount in listOf("0", "-5", "ten")) {
            val request = topUpRequest(NativeTopUpSource.PrivateKey(key)).copy(amount = amount)
            val error = runCatching { platforms.topUp(product, request) }.exceptionOrNull()

            assertTrue("$amount: $error", error is HostPaymentTopUpException.InvalidAmount)
        }
    }

    @Test
    fun `an unexpected top-up failure reaches the product without the engine's words`() = runBlocking<Unit> {
        val key = ByteArray(64) { 1 }
        coEvery { topUpService.start(product, topUpId, planks(25), any()) } returns
            Result.failure(IllegalStateException("coin 0x12 locked by claim 7"))

        val error = runCatching { platforms.topUp(product, topUpRequest(NativeTopUpSource.PrivateKey(key))) }
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

        val error = runCatching { platforms.requestPayment(product, paymentRequest()) }.exceptionOrNull()

        assertTrue(error is HostPaymentException.InsufficientBalance)
    }

    @Test
    fun `following a payment the host never registered is not found`() = runBlocking<Unit> {
        every { requestPaymentUseCase.subscribeStatus(product, paymentId) } returns
            flow { throw PaymentRequestError.NotFound(paymentId) }

        val error = runCatching { platforms.followPayment(product, rawId) }.exceptionOrNull()

        assertTrue("$error", error is HostPaymentStatusSubscribeException.PaymentNotFound)
    }

    @Test
    fun `a followed payment is pushed until it settles`() = runBlocking<Unit> {
        every { requestPaymentUseCase.subscribeStatus(product, paymentId) } returns
            flowOf(PaymentStatus.Processing, PaymentStatus.PartiallyClaimed(planks(9)), PaymentStatus.Completed)

        platforms.followPayment(product, rawId)

        assertEquals(
            listOf(
                HostPaymentStatusSubscribeItem.Processing,
                HostPaymentStatusSubscribeItem.ClaimedPartially(actualClaimed = "9"),
            ),
            pushedPayments,
        )
    }

    @Test
    fun `a payment of nothing, or of no number, is an invalid amount`() = runBlocking<Unit> {
        for (amount in listOf("0", "-5", "ten")) {
            val error = runCatching { platforms.requestPayment(product, paymentRequest().copy(amount = amount)) }
                .exceptionOrNull()

            assertTrue("$amount: $error", error is HostPaymentException.InvalidAmount)
        }
    }

    @Test
    fun `an unexpected payment failure reaches the product without the engine's words`() = runBlocking<Unit> {
        coEvery { requestPaymentUseCase.requestPayment(product, paymentId, planks(25), any(), any()) } returns
            Result.failure(IllegalStateException("cheque store at /data/coinage is corrupt"))

        val error = runCatching { platforms.requestPayment(product, paymentRequest()) }.exceptionOrNull()

        assertTrue(error is HostPaymentException.Unknown)
        assertEquals("payment could not be started", (error as HostPaymentException.Unknown).reason)
    }

    /** The engine's reason can name a shortfall the core masks, so a failed payment reaches the product generic. */
    @Test
    fun `a failed payment reaches the product with a fixed reason`() = runBlocking<Unit> {
        every { requestPaymentUseCase.subscribeStatus(product, paymentId) } returns
            flowOf(PaymentStatus.Processing, PaymentStatus.Failed("only 12 planks spendable"))

        platforms.followPayment(product, rawId)

        assertEquals(
            listOf(
                HostPaymentStatusSubscribeItem.Processing,
                HostPaymentStatusSubscribeItem.Failed(reason = "payment did not complete"),
            ),
            pushedPayments,
        )
    }

    @Test
    fun `a payment whose engine fails while processing fails the follow with a fixed reason`() = runBlocking<Unit> {
        every { requestPaymentUseCase.subscribeStatus(product, paymentId) } returns flow {
            emit(PaymentStatus.Processing)
            throw IllegalStateException("cheque store at /data/coinage is corrupt")
        }

        val error = runCatching { platforms.followPayment(product, rawId) }.exceptionOrNull()

        assertEquals(listOf(HostPaymentStatusSubscribeItem.Processing), pushedPayments)
        assertEquals("status is unavailable", (error as HostPaymentStatusSubscribeException.Unknown).reason)
    }

    // ---- balance ----

    @Test
    fun `the balance is what a payment can spend, and each change is pushed`() = runBlocking<Unit> {
        every { totalBalanceUseCase.subscribeTotalBalance() } returns flowOf(
            Result.success(balance(availablePrivate = 10, gainingPrivacy = 5)),
            Result.success(balance(availablePrivate = 10, gainingPrivacy = 5)),
            Result.success(balance(availablePrivate = 12, gainingPrivacy = 5)),
        )

        platforms.followBalance()

        assertEquals(listOf(planks(15), planks(17)), pushedBalances)
    }

    @Test
    fun `a balance stream that fails ends the follow without the engine's words`() = runBlocking<Unit> {
        every { totalBalanceUseCase.subscribeTotalBalance() } returns flow { throw IllegalStateException("coinage not set up") }

        val error = runCatching { platforms.followBalance() }.exceptionOrNull()

        assertEquals("balance is unavailable", (error as HostRejection.Rejected).reason)
    }

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
