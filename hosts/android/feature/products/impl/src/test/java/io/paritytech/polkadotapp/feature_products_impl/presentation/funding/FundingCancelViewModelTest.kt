package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModelEvent
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCash
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCountry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingLimits
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayContext
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayContexts
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayMode
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingBand
import uniffi.truapi.FundingCancel
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingStatus
import io.paritytech.polkadotapp.common.R as RCommon

/** A started session the user cancels from its deposit screen, whatever the core and the provider then answer. */
class FundingCancelViewModelTest {
    private val testDispatcher = StandardTestDispatcher()
    private val request = FundingOverlayRequest(productId = null, intent = "intent", direction = FundingDirection.OUT, amount = null)
    private val context = FundingOverlayContext(request, FundingOverlayMode.Resumed)
    private val contexts = FundingOverlayContexts().apply { put(context) }
    private val router = mockk<ProductsRouter>(relaxed = true)
    private val sessionChanges = MutableSharedFlow<Unit>()
    private var cancel: FundingCancel? = null
    private var moneyMoved = false
    private val models = mutableListOf<FundingViewModel>()

    private val interactor = mockk<FundingFlowInteractor> {
        coEvery { cash() } returns FundingCash(symbol = "CASH", precision = 6)
        coEvery { spendable(any()) } returns null
        coEvery { candidates("intent") } returns emptyList()
        coEvery { detectedCountry() } returns FundingCountry("DE", "Germany", "EUR", "Euro")
        every { countries() } returns emptyList()
        every { quoteLists("intent") } returns MutableSharedFlow()
        coEvery { countrySupport("intent", any(), any()) } returns emptyList()
        coEvery { stopQuotes("intent") } returns Unit
        every { sessionChanges("intent") } returns sessionChanges
        coEvery { requestQuote("intent", any()) } returns Result.success(1uL)
        coEvery { amountLimits("intent", any(), any(), any(), any(), any()) } returns FundingLimits(null, null)
        coEvery { brand(any()) } answers { FundingProviderBrand(firstArg(), firstArg(), null) }
        coEvery { progress("intent") } answers { progress() }
        coEvery { session("intent") } answers { session() }
    }

    @Before
    fun setUp() = Dispatchers.setMain(testDispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    // The deposit screen polls until the sheet goes. If an assertion throws first, runTest keeps draining that loop
    // in virtual time, so every model is stopped before runTest finishes, whatever the test did.
    private fun runCancelTest(body: suspend TestScope.() -> Unit) = runTest(testDispatcher) {
        try {
            body()
        } finally {
            models.forEach(::stop)
        }
    }

    /** The core sends the first cancel to the provider; the second, before money moved, ends the session. */
    @Test
    fun `before money moved, cancelling again ends a session a silent provider holds`() = runCancelTest {
        coEvery { interactor.cancel("intent") } returns Result.success(CancelOutcome.REQUESTED)
        val model = resumed()

        model.onCancelTopUp()
        model.onConfirmCancel()
        cancel = FundingCancel.Requested
        sessionChanges.emit(Unit)
        runCurrent()

        assertEquals(FundingCancelUiState.Requested, deposit(model).cancel)
        assertTrue(deposit(model).canCancel)
        assertTrue(deposit(model).isSecondCancel)

        coEvery { interactor.cancel("intent") } returns Result.success(CancelOutcome.ENDED)
        model.onCancelTopUp()
        model.onConfirmCancel()
        runCurrent()

        coVerify(exactly = 2) { interactor.cancel("intent") }
        coVerify { router.closeFundingOverlay("intent") }
        stop(model)
    }

    @Test
    fun `once money moved, a cancel with the provider can't be sent again`() = runCancelTest {
        moneyMoved = true
        cancel = FundingCancel.Requested
        val model = resumed()

        assertEquals(FundingCancelUiState.Requested, deposit(model).cancel)
        assertFalse(deposit(model).canCancel)
        assertFalse(deposit(model).isSecondCancel)
        stop(model)
    }

    @Test
    fun `a cancel the provider decides goes back to the deposit and says it was requested`() = runCancelTest {
        for (outcome in listOf(CancelOutcome.REQUESTED, CancelOutcome.PENDING)) {
            coEvery { interactor.cancel("intent") } returns Result.success(outcome)
            val model = resumed()

            val messages = messages(model)
            model.onCancelTopUp()
            model.onConfirmCancel()
            runCurrent()

            assertEquals(FundingScreen.DEPOSIT, screen(model))
            assertEquals(listOf(RCommon.string.funding_cancel_requested), messages)
            stop(model)
        }
    }

    @Test
    fun `a top-up already credited is too late to cancel`() = runCancelTest {
        coEvery { interactor.cancel("intent") } returns Result.success(CancelOutcome.TOO_LATE)
        val model = resumed()

        val messages = messages(model)
        model.onCancelTopUp()
        model.onConfirmCancel()
        runCurrent()

        assertEquals(FundingScreen.DEPOSIT, screen(model))
        assertEquals(listOf(RCommon.string.funding_cancel_too_late), messages)
        coVerify(exactly = 0) { router.closeFundingOverlay(any()) }
        stop(model)
    }

    @Test
    fun `a cancel the core refuses says it can't be cancelled now`() = runCancelTest {
        coEvery { interactor.cancel("intent") } returns Result.success(CancelOutcome.REFUSED)
        val model = resumed()

        val messages = messages(model)
        model.onCancelTopUp()
        model.onConfirmCancel()
        runCurrent()

        assertEquals(listOf(RCommon.string.funding_error_cancel_refused), messages)
        stop(model)
    }

    @Test
    fun `the provider's offer opens natively with its fee and refund, and accepting answers the core`() = runCancelTest {
        coEvery { interactor.confirmCancel("intent", CancelTerms(fee = "2500000", refund = "1000250000")) } returns
            Result.success(CancelConfirmation.ACCEPTED)
        val model = resumed()

        cancel = FundingCancel.Offered(fee = "2500000", refund = "1000250000", expiresAt = null)
        sessionChanges.emit(Unit)
        runCurrent()

        assertEquals(FundingScreen.CANCEL_OFFER, screen(model))
        assertEquals(FundingCancelUiState.Offered(fee = "$2.50 CASH", refund = "$1,000.25 CASH"), deposit(model).cancel)
        assertFalse(deposit(model).canCancel)

        val messages = messages(model)
        cancel = FundingCancel.Accepted(fee = "2500000", refund = "1000250000")
        model.onAcceptCancelOffer()
        runCurrent()

        coVerify { interactor.confirmCancel("intent", CancelTerms(fee = "2500000", refund = "1000250000")) }
        assertEquals(FundingScreen.DEPOSIT, screen(model))
        assertEquals(FundingCancelUiState.Cancelling, deposit(model).cancel)
        assertEquals(listOf(RCommon.string.funding_cancel_accepted), messages)
        stop(model)
    }

    @Test
    fun `declining the offer keeps the session going with no message`() = runCancelTest {
        coEvery { interactor.confirmCancel("intent", null) } returns Result.success(CancelConfirmation.DECLINED)
        cancel = FundingCancel.Offered(fee = "1000000", refund = "9000000", expiresAt = null)
        val model = resumed()
        assertEquals(FundingScreen.CANCEL_OFFER, screen(model))

        val messages = messages(model)
        cancel = FundingCancel.Declined
        model.onDeclineCancelOffer()
        runCurrent()

        assertEquals(FundingScreen.DEPOSIT, screen(model))
        assertEquals(null, deposit(model).cancel)
        assertTrue(deposit(model).canCancel)
        assertTrue(messages.isEmpty())
        stop(model)
    }

    @Test
    fun `a lapsed or vanished offer says so`() = runCancelTest {
        val cases = mapOf(
            CancelConfirmation.LAPSED to RCommon.string.funding_cancel_offer_lapsed,
            CancelConfirmation.NO_OFFER to RCommon.string.funding_cancel_offer_gone,
            CancelConfirmation.TOO_LATE to RCommon.string.funding_cancel_too_late,
        )
        for ((answer, message) in cases) {
            coEvery { interactor.confirmCancel("intent", any()) } returns Result.success(answer)
            cancel = FundingCancel.Offered(fee = "1000000", refund = "9000000", expiresAt = null)
            val model = resumed()

            val messages = messages(model)
            model.onAcceptCancelOffer()
            runCurrent()

            assertEquals(listOf(message), messages)
            stop(model)
        }
    }

    @Test
    fun `a refusal shows the provider's reason and lets the user ask again`() = runCancelTest {
        moneyMoved = true
        cancel = FundingCancel.Requested
        val model = resumed()
        assertEquals(FundingCancelUiState.Requested, deposit(model).cancel)
        assertFalse(deposit(model).canCancel)

        cancel = FundingCancel.Refused(reason = "the payout already left")
        sessionChanges.emit(Unit)
        runCurrent()

        assertEquals(FundingCancelUiState.Refused("the payout already left"), deposit(model).cancel)
        assertTrue(deposit(model).canCancel)
        stop(model)
    }

    private fun session(): FundingSession = mockk(relaxed = true) {
        every { stage } returns FundingStage.Open
        every { choice } returns null
        every { providerId } returns null
        every { cancel } answers { this@FundingCancelViewModelTest.cancel }
    }

    private fun progress() = FundingProgress(
        steps = emptyList(),
        band = FundingBand.AWAITING_PAYMENT,
        lastHeardAtMs = null,
        failedAtMs = null,
        transactionId = null,
        reference = null,
        deposit = null,
        mismatch = null,
        retrying = false,
        payout = null,
        refunded = null,
        status = FundingStatus.WaitingForPayment,
        longerAtMs = null,
        moneyMoved = moneyMoved,
    )

    private fun TestScope.resumed(): FundingViewModel {
        val model = FundingViewModel(context, contexts, interactor, router, mockk(relaxed = true))
        models += model
        runCurrent()
        return model
    }

    private fun TestScope.messages(model: FundingViewModel): List<Int> {
        val collected = mutableListOf<Int>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            model.events.collect { event -> if (event is BaseViewModelEvent.ResourceMessage) collected += event.messageRes }
        }
        return collected
    }

    private suspend fun screen(model: FundingViewModel): FundingScreen = loaded(model).screen

    private suspend fun deposit(model: FundingViewModel): FundingDepositUiState = loaded(model).deposit

    private suspend fun loaded(model: FundingViewModel): FundingSheetUiState =
        (model.state.first { it is LoadingState.Loaded } as LoadingState.Loaded).data

    /** The deposit screen keeps reading the session until the sheet goes, which in a test it never does. */
    private fun stop(model: FundingViewModel) {
        model.coroutineContext.cancelChildren()
    }
}
