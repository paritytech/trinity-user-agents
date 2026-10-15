package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCash
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCountry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingLimits
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingKey
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayContext
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayContexts
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayMode
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Before
import org.junit.Test
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingRoute
import java.math.BigDecimal
import uniffi.truapi.FundingDirection as CoreFundingDirection

class FundingViewModelTest {
    private val testDispatcher = StandardTestDispatcher()
    private val request = FundingOverlayRequest(productId = null, intent = "intent", direction = FundingDirection.IN, amount = null)
    private val context = FundingOverlayContext(request, FundingOverlayMode.New)
    private val contexts = FundingOverlayContexts().apply { put(context) }
    private val router = mockk<ProductsRouter>(relaxed = true)
    private val quoteLists = MutableSharedFlow<FundingQuoteList>()
    private val interactor = mockk<FundingFlowInteractor> {
        coEvery { cash() } returns FundingCash(symbol = "CASH", precision = 6)
        coEvery { spendable(any()) } returns null
        coEvery { candidates("intent") } returns listOf(cardCandidate())
        coEvery { detectedCountry() } returns FundingCountry("DE", "Germany", "EUR", "Euro")
        every { countries() } returns emptyList()
        every { quoteLists("intent") } returns quoteLists
        coEvery { countrySupport("intent", any(), any()) } returns emptyList()
        coEvery { stopQuotes("intent") } returns Unit
        every { sessionChanges("intent") } returns emptyFlow()
        coEvery { requestQuote("intent", any()) } returns Result.success(1uL)
        coEvery { amountLimits("intent", any(), any(), any(), any(), any()) } returns FundingLimits(null, null)
        coEvery { brand(any()) } answers { FundingProviderBrand(firstArg(), firstArg(), null) }
        coEvery { selectProvider("intent", "ramp.dot", "q-1") } returns Result.success(true)
        coEvery { progress("intent") } returns null
        coEvery { session("intent") } returns null
    }

    @Before
    fun setUp() = Dispatchers.setMain(testDispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun `the amount is quoted once the user stops typing for 0_6 s`() = runTest(testDispatcher) {
        val model = viewModel()
        advanceUntilIdle()

        model.onKey(FundingKey.Digit('5'))
        advanceTimeBy(300)
        model.onKey(FundingKey.Digit('0'))
        advanceTimeBy(599)
        runCurrent()
        coVerify(exactly = 0) { interactor.requestQuote(any(), any()) }

        advanceTimeBy(2)
        runCurrent()
        coVerify(exactly = 1) {
            interactor.requestQuote(
                "intent",
                FundingQuoteAsk(CoreFundingDirection.IN, FundingRail.CARD, "EUR", null, "50000000", "DE"),
            )
        }
    }

    @Test
    fun `closing before a provider was chosen answers dismissed`() = runTest(testDispatcher) {
        val model = viewModel()
        advanceUntilIdle()
        val answer = async { context.awaitOutcome() }

        model.onClose()
        advanceUntilIdle()

        assertEquals(FundingOverlayOutcome.DISMISSED, answer.await())
        // The core asks every provider again each minute until told otherwise.
        coVerify { interactor.stopQuotes("intent") }
    }

    @Test
    fun `the overlay answers started only once the provider took the session, and a card closes it`() = runTest(testDispatcher) {
        val model = viewModel()
        advanceUntilIdle()
        val answer = async { context.awaitOutcome() }

        model.onPreset(BigDecimal("50"))
        advanceTimeBy(601)
        runCurrent()
        quoteLists.emit(list(FundingQuoteRow("ramp.dot", FundingQuoteState.Quoted(quote())), best = "ramp.dot"))
        model.onContinueFromAmount()
        runCurrent()
        assertFalse(answer.isCompleted)

        model.onStart()
        runCurrent()

        assertEquals(FundingOverlayOutcome.STARTED, answer.await())
        coVerify { router.closeFundingOverlay("intent") }
        cancelTicker(model)
    }

    @Test
    fun `a crypto top-up starts with the best quote once every provider answered`() = runTest(testDispatcher) {
        coEvery { interactor.candidates("intent") } returns listOf(cryptoCandidate("a"), cryptoCandidate("b"))
        coEvery { interactor.selectProvider("intent", "b", "q-b") } returns Result.success(true)
        val model = viewModel()
        advanceUntilIdle()
        val answer = async { context.awaitOutcome() }

        model.onPreset(BigDecimal("50"))
        model.onContinueFromAmount()
        model.onNetworkChosen("ethereum")
        model.onTokenChosen("USDT")
        runCurrent()
        val quotedA = FundingQuoteRow("a", FundingQuoteState.Quoted(quote().copy(quoteId = "q-a", sendAmount = "52000000")))
        quoteLists.emit(list(quotedA, FundingQuoteRow("b", FundingQuoteState.Pending), best = "a"))
        runCurrent()
        assertFalse(answer.isCompleted)

        quoteLists.emit(list(quotedA, FundingQuoteRow("b", FundingQuoteState.Quoted(quote().copy(quoteId = "q-b", sendAmount = "51800000"))), best = "b"))
        runCurrent()

        assertEquals(FundingOverlayOutcome.STARTED, answer.await())
        coVerify(exactly = 0) { router.closeFundingOverlay(any()) }
        stopFollowingDeposit(model)
    }

    private fun cryptoCandidate(providerId: String) = FundingCandidate(
        providerId = providerId,
        routes = listOf(
            FundingRoute(
                rail = FundingRail.CRYPTO,
                directions = listOf(CoreFundingDirection.IN),
                assets = listOf("USDT"),
                networks = listOf("ethereum"),
                countries = null,
            ),
        ),
        unsupported = emptyList(),
        limits = emptyList(),
    )

    /** The deposit screen keeps reading the session until the sheet goes, which in a test it never does. */
    private fun stopFollowingDeposit(model: FundingViewModel) {
        model.coroutineContext.cancelChildren()
    }

    private fun TestScope.cancelTicker(model: FundingViewModel) {
        model.onBack()
        advanceUntilIdle()
    }

    private fun quote() = FundingQuote(
        quoteId = "q-1",
        sendAmount = "5100",
        receiveAmount = "50000000",
        providerFee = "90",
        networkFee = "10",
        etaSecs = 120uL,
        expiresAt = null,
        assetDecimals = 2u,
    )

    private fun list(vararg rows: FundingQuoteRow, best: String?) =
        FundingQuoteList(requestId = 1uL, rows = rows.toList(), best = best, refreshAtMs = 60_000uL)

    private fun viewModel() = FundingViewModel(context, contexts, interactor, router, mockk(relaxed = true))

    private fun cardCandidate() = FundingCandidate(
        providerId = "ramp.dot",
        routes = listOf(
            FundingRoute(
                rail = FundingRail.CARD,
                directions = listOf(CoreFundingDirection.IN),
                assets = listOf("EUR"),
                networks = null,
                countries = null,
            ),
        ),
        unsupported = emptyList(),
        limits = emptyList(),
    )
}
