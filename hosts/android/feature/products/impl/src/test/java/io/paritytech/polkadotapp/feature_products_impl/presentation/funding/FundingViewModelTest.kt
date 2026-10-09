package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.verify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCash
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCountry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingKey
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayContext
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayContexts
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
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
import uniffi.truapi.FundingMode
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingRoute
import uniffi.truapi.RouteDirection
import java.math.BigDecimal
import uniffi.truapi.FundingDirection as CoreFundingDirection

class FundingViewModelTest {
    private val testDispatcher = StandardTestDispatcher()
    private val request = FundingOverlayRequest(productId = null, intent = "intent", direction = FundingDirection.IN, amount = null)
    private val context = FundingOverlayContext(request)
    private val contexts = FundingOverlayContexts().apply { put(context) }
    private val router = mockk<ProductsRouter>(relaxed = true)
    private val quoteRows = MutableSharedFlow<FundingQuoteRow>()
    private val interactor = mockk<FundingFlowInteractor> {
        coEvery { cash() } returns FundingCash(symbol = "CASH", precision = 6)
        coEvery { spendable(any()) } returns null
        coEvery { candidates("intent") } returns listOf(cardCandidate())
        every { detectedCountry() } returns FundingCountry("DE", "Germany", "EUR", "Euro")
        every { quoteRows("intent") } returns quoteRows
        every { sessionChanges("intent") } returns emptyFlow()
        coEvery { requestQuote("intent", any()) } returns Result.success(Unit)
        coEvery { brand(any()) } answers { FundingProviderBrand(firstArg(), firstArg(), null) }
        coEvery { selectProvider("intent", "ramp.dot", "q-1") } returns Result.success(true)
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
    }

    @Test
    fun `the overlay answers started only once the provider took the session, and a card closes it`() = runTest(testDispatcher) {
        val model = viewModel()
        advanceUntilIdle()
        val answer = async { context.awaitOutcome() }

        model.onPreset(BigDecimal("50"))
        advanceTimeBy(601)
        runCurrent()
        quoteRows.emit(FundingQuoteRow("ramp.dot", FundingQuoteState.Quoted(quote())))
        model.onContinueFromAmount()
        runCurrent()
        assertFalse(answer.isCompleted)

        model.onStart()
        runCurrent()

        assertEquals(FundingOverlayOutcome.STARTED, answer.await())
        verify { router.back() }
        cancelTicker(model)
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
    )

    private fun viewModel() = FundingViewModel(context, contexts, interactor, router)

    private fun cardCandidate() = FundingCandidate(
        providerId = "ramp.dot",
        routes = listOf(
            FundingRoute(
                mode = FundingMode.CARD,
                directions = listOf(RouteDirection.IN),
                assets = listOf("EUR"),
                networks = null,
                countries = null,
                requiresAccount = false,
            ),
        ),
        unsupported = emptyList(),
        limits = emptyList(),
        backend = null,
    )
}
