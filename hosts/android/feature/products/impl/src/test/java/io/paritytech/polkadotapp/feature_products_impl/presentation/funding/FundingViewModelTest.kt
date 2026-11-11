package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import androidx.lifecycle.ViewModelStore
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCash
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
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
import kotlinx.coroutines.flow.MutableStateFlow
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
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.truapi.FundingAmountLimits
import uniffi.truapi.FundingBookEntry
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingChoice
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingRoute
import uniffi.truapi.FundingSelection
import uniffi.truapi.ProductId
import java.math.BigDecimal
import uniffi.truapi.FundingDirection as CoreFundingDirection

class FundingViewModelTest {
    private val ramp = ProductId.parse("ramp.dot").getOrThrow()
    private val providerA = ProductId.parse("a.dot").getOrThrow()
    private val providerB = ProductId.parse("b.dot").getOrThrow()
    private val testDispatcher = StandardTestDispatcher()
    private val request = FundingOverlayRequest(productId = null, intent = "intent", direction = FundingDirection.IN, amount = null)
    private val context = FundingOverlayContext(request, FundingOverlayMode.New)
    private val contexts = FundingOverlayContexts().apply { put(context) }
    private val router = mockk<ProductsRouter>(relaxed = true)
    private val quoteLists = MutableSharedFlow<FundingQuoteList?>()
    private val session = MutableStateFlow<FundingBookEntry?>(null)
    private val pushedOptions = MutableStateFlow<FundingOptions?>(options(cardCandidate()))
    private val interactor = mockk<FundingFlowInteractor> {
        coEvery { cash() } returns FundingCash(symbol = "CASH", precision = 6)
        coEvery { spendable(any()) } returns null
        every { options("intent") } returns pushedOptions
        coEvery { selectOptions("intent", any()) } returns Unit
        every { countries() } returns emptyList()
        every { quoteLists("intent") } returns quoteLists
        every { session("intent") } returns session
        coEvery { requestQuote("intent", any(), any()) } returns Result.success(1uL)
        coEvery { amountIssue("intent", any(), any()) } returns null
        coEvery { startWhenQuoted("intent", any()) } returns Unit
        coEvery { cancelAutoStart("intent") } returns Unit
        coEvery { brand(any()) } answers { FundingProviderBrand(firstArg(), firstArg<ProductId>().value, null) }
        coEvery { selectProvider("intent", ramp, "q-1") } returns Result.success(true)
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
        coVerify(exactly = 0) { interactor.requestQuote(any(), any(), any()) }

        advanceTimeBy(2)
        runCurrent()
        coVerify(exactly = 1) {
            interactor.requestQuote("intent", FundingSelection(FundingRail.CARD, null, null, "DE", "EUR"), "50000000")
        }
    }

    // The sheet can open again for the session before the old one's view model is cleared, and the new sheet's
    // context is the one the overlay answers through.
    @Test
    fun `clearing leaves a newer sheet's context for the session in place`() = runTest(testDispatcher) {
        val model = viewModel()
        advanceUntilIdle()
        val newer = FundingOverlayContext(request, FundingOverlayMode.Resumed)
        contexts.put(newer)

        ViewModelStore().apply { put("funding", model) }.clear()

        assertSame(newer, contexts.get("intent"))
    }

    @Test
    fun `clearing drops the sheet's own context`() = runTest(testDispatcher) {
        val model = viewModel()
        advanceUntilIdle()

        ViewModelStore().apply { put("funding", model) }.clear()

        assertNull(contexts.get("intent"))
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
        quoteLists.emit(list(FundingQuoteRow(ramp, FundingQuoteState.Quoted(quote())), best = ramp))
        model.onContinueFromAmount()
        runCurrent()
        assertFalse(answer.isCompleted)

        model.onStart()
        runCurrent()

        assertEquals(FundingOverlayOutcome.STARTED, answer.await())
        coVerify { router.closeFundingOverlay("intent") }
        cancelTicker(model)
    }

    // The core starts a crypto top-up on its best quote once every provider answered; the deposit screen asks it to.
    @Test
    fun `a crypto top-up asks the core to start on the best quote, and answers started once it has`() = runTest(testDispatcher) {
        pushedOptions.value = options(cryptoCandidate(providerA), cryptoCandidate(providerB))
        val model = viewModel()
        advanceUntilIdle()
        val answer = async { context.awaitOutcome() }

        model.onPreset(BigDecimal("50"))
        model.onContinueFromAmount()
        model.onNetworkChosen("ethereum")
        model.onTokenChosen("USDT")
        runCurrent()
        coVerify { interactor.startWhenQuoted("intent", 1uL) }
        assertFalse(answer.isCompleted)

        session.value = FundingBookEntry(startedSession(providerB, FundingRail.CRYPTO), awaitingDeposit())
        runCurrent()

        assertEquals(FundingOverlayOutcome.STARTED, answer.await())
        coVerify(exactly = 0) { interactor.selectProvider(any(), any(), any()) }
        coVerify(exactly = 0) { router.closeFundingOverlay(any()) }
        stopFollowingDeposit(model)
    }

    @Test
    fun `going back from a crypto top-up's deposit screen disarms its start`() = runTest(testDispatcher) {
        pushedOptions.value = options(cryptoCandidate(providerA))
        val model = viewModel()
        advanceUntilIdle()

        model.onPreset(BigDecimal("50"))
        model.onContinueFromAmount()
        model.onNetworkChosen("ethereum")
        model.onTokenChosen("USDT")
        runCurrent()
        model.onBack()
        runCurrent()

        coVerify { interactor.cancelAutoStart("intent") }
        assertFalse(context.isClosed)
        stopFollowingDeposit(model)
    }

    @Test
    fun `closing marks the sheet closed, so one left under another screen closes on its return`() = runTest(testDispatcher) {
        val model = viewModel()
        advanceUntilIdle()

        model.onClose()
        advanceUntilIdle()

        assertTrue(context.isClosed)
        coVerify { router.closeFundingOverlay("intent") }
    }

    @Test
    fun `the core is told each selection, and the amount limits it pushes back reach the amount screen`() = runTest(testDispatcher) {
        val model = viewModel()
        advanceUntilIdle()
        coVerify { interactor.selectOptions("intent", FundingSelection(FundingRail.CARD, null, null, "DE", "EUR")) }

        pushedOptions.value = options(cardCandidate(), min = "20000000")
        advanceUntilIdle()

        val note = (model.state.value as LoadingState.Loaded).data.amount.note
        assertEquals(FundingAmountNote.Minimum(FundingCash("CASH", 6).label(BigDecimal("20")), withdraw = false), note)
        cancelTicker(model)
    }

    // What the core offers: card with the user's currency, or crypto, on the country it detected.
    private fun options(vararg candidates: FundingCandidate, min: String? = null): FundingOptions {
        val rail = candidates.first().routes.first().rail
        return FundingOptions(
            candidates = candidates.toList(),
            countries = emptyList(),
            limits = FundingAmountLimits(min = min, max = null),
            networks = emptyList(),
            rails = listOf(rail),
            asset = "EUR".takeIf { rail == FundingRail.CARD },
            tokens = emptyList(),
            detectedCountry = "DE",
        )
    }

    private fun cryptoCandidate(providerId: ProductId) = FundingCandidate(
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
        declaresRoutes = true,
        brand = null,
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

    private fun list(
        vararg rows: FundingQuoteRow,
        best: ProductId?,
        requestId: ULong = 1uL,
        refreshAtMs: ULong = 60_000uL,
    ) = FundingQuoteList(
        requestId = requestId,
        rows = rows.toList(),
        best = best,
        refreshAtMs = refreshAtMs,
        amount = "50000000",
        refusal = null,
        issue = null,
        complete = rows.none { it.state is FundingQuoteState.Pending },
    )

    // ULong fields read from a relaxed mock come back as Long, so the ones the deposit screen reads are stubbed.
    private fun awaitingDeposit(): FundingProgress = mockk(relaxed = true) {
        every { lastHeardAtMs } returns null
        every { failedAtMs } returns null
        every { longerAtMs } returns null
    }

    private fun startedSession(providerId: ProductId, rail: FundingRail): FundingSession = mockk(relaxed = true) {
        every { this@mockk.providerId } returns providerId
        every { stage } returns FundingStage.Open
        every { choice } returns mockk<FundingChoice>(relaxed = true) { every { this@mockk.rail } returns rail }
    }

    private fun viewModel() = FundingViewModel(context, contexts, interactor, router, mockk(relaxed = true))

    private fun cardCandidate() = FundingCandidate(
        providerId = ramp,
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
        declaresRoutes = true,
        brand = null,
    )
}
