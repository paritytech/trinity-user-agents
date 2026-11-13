package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FakeFundingCore
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingStartError
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingFixtures.entry
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingFixtures.flow
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingFixtures.ramp
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingAmountViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingCancelConfirmViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingCancelOfferViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingCountryViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingSummaryViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingTokenViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingCancel
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingRail
import io.paritytech.polkadotapp.common.R as RCommon

class FundingScreenViewModelsTest {
    private val testDispatcher = StandardTestDispatcher()
    private val service = FakeFundingService()

    @Before
    fun setUp() = Dispatchers.setMain(testDispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun `the amount is quoted once the user stops typing for 0_6 s`() = runTest(testDispatcher) {
        val sheet = FakeFundingSheet(flow(), entry())
        val model = FundingAmountViewModel(sheet, service)

        model.onKey(FundingKey.Digit('5'))
        advanceTimeBy(300)
        model.onKey(FundingKey.Digit('0'))
        advanceTimeBy(599)
        runCurrent()
        assertEquals(emptyList<String>(), sheet.actions)

        advanceTimeBy(2)
        runCurrent()
        assertEquals(listOf("requote"), sheet.actions)
        assertEquals("50", sheet.flow.value?.amountText)
    }

    @Test
    fun `continuing with a quote still waiting asks it at once and shows the summary`() = runTest(testDispatcher) {
        val sheet = FakeFundingSheet(flow(), entry())
        val model = FundingAmountViewModel(sheet, service)
        model.state.launchIn(backgroundScope)
        model.onKey(FundingKey.Digit('5'))
        runCurrent()

        model.onContinue()
        advanceUntilIdle()

        assertEquals(listOf("requote", "push SUMMARY"), sheet.actions)
    }

    @Test
    fun `switching rail drops the quotes on screen`() = runTest(testDispatcher) {
        val sheet = FakeFundingSheet(flow(), entry())
        val model = FundingAmountViewModel(sheet, service)

        model.onRailSelected(FundingRail.BANK)

        assertEquals(listOf("quote null"), service.calls)
        assertEquals(FundingRail.BANK, sheet.flow.value?.rail)
    }

    // The amount screen's state is all a view model on it builds; the country list waits for its own screen.
    @Test
    fun `only the screen on the sheet builds its state`() = runTest(testDispatcher) {
        val sheet = FakeFundingSheet(flow(), entry())
        val country = FundingCountryViewModel(sheet)
        advanceUntilIdle()
        assertNull(country.state.value)

        country.state.launchIn(backgroundScope)
        runCurrent()
        assertEquals("", country.state.value?.query)
    }

    @Test
    fun `starting hands the session to the quoted provider, and a refusal lets the user try again`() = runTest(testDispatcher) {
        val sheet = FakeFundingSheet(flow().withQuotes(quotedByRamp(), emptyMap(), asking = false), entry())
        val model = FundingSummaryViewModel(sheet, service)
        model.state.launchIn(backgroundScope)
        service.startResult = Result.failure(FundingStartError.NotSelected(null))

        model.onStart()
        advanceUntilIdle()

        assertEquals("start ramp.dot", service.calls.last())
        assertEquals(FundingSummaryProblem.StartFailed, model.state.value?.problem)
        assertTrue(model.state.value?.canStart == true)

        service.startResult = Result.success(Unit)
        model.onStart()
        advanceUntilIdle()
        assertEquals(false, model.state.value?.canStart)
    }

    @Test
    fun `a crypto top-up or withdrawal goes to its summary, and starts only from there`() = runTest(testDispatcher) {
        val topUp = FakeFundingSheet(flow(), entry())
        FundingTokenViewModel(topUp).onTokenChosen("USDC")
        advanceUntilIdle()

        assertEquals(listOf("requote", "push SUMMARY"), topUp.actions)
        assertEquals(emptyList<String>(), service.calls)
        assertEquals("USDC", topUp.flow.value?.asset)

        val withdrawal = FakeFundingSheet(flow().copy(direction = FundingDirection.OUT), entry())
        FundingTokenViewModel(withdrawal).onTokenChosen("USDC")
        assertEquals(listOf("requote", "push SUMMARY"), withdrawal.actions)
    }

    @Test
    fun `a cancel before money moved leaves the sheet, one after it waits on the provider`() = runTest(testDispatcher) {
        val sheet = FakeFundingSheet(flow(), entry(session = FakeFundingCore.session("intent", providerId = ramp)))
        val model = FundingCancelConfirmViewModel(sheet, service)

        service.cancelResult = Result.success(CancelOutcome.ENDED)
        model.onConfirm()
        advanceUntilIdle()
        assertEquals(listOf("close"), sheet.actions)

        sheet.actions.clear()
        service.cancelResult = Result.success(CancelOutcome.REQUESTED)
        model.onConfirm()
        advanceUntilIdle()
        assertEquals(listOf("pop CANCEL_CONFIRM"), sheet.actions)
        assertEquals(listOf(RCommon.string.funding_cancel_requested), sheet.messages)
    }

    // Accepting names the terms on screen, so the core accepts nothing but the offer the user saw.
    @Test
    fun `accepting the provider's offer answers with the terms shown`() = runTest(testDispatcher) {
        val offered = FakeFundingCore.session("intent", providerId = ramp).copy(cancel = FundingCancel.Offered("1", "9", null))
        val sheet = FakeFundingSheet(flow(), entry(session = offered))
        val model = FundingCancelOfferViewModel(sheet, service)
        service.answerResult = Result.success(CancelConfirmation.ACCEPTED)

        model.onAccept()
        advanceUntilIdle()

        assertEquals(CancelTerms(fee = "1", refund = "9"), service.answeredTerms)
        assertEquals(listOf("pop CANCEL_OFFER"), sheet.actions)
        assertEquals(listOf(RCommon.string.funding_cancel_accepted), sheet.messages)
    }

    private fun quotedByRamp() = FakeFundingCore.list(1uL, "50000000", ramp to FundingQuoteState.Quoted(FakeFundingCore.quote("q-1")))
}
