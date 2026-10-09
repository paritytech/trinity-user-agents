package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingBalance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.FundingChoice
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingProgressStep
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingStep
import uniffi.truapi.FundingDirection as CoreFundingDirection

class AppFundingOverlayTest {
    private val contexts = FundingOverlayContexts()
    private val frames = FundingFrameContexts()
    private val runtime = mockk<FundingRuntime>()
    private val router = mockk<ProductsRouter>(relaxed = true)
    private val overlay = AppFundingOverlay(contexts, frames, runtime, router)

    @Test
    fun `a card's provider screen opens once its sheet has closed`() = runTest {
        coEvery { runtime.session("intent") } returns session(FundingRail.CARD, FundingStage.Open)
        val sheet = FundingOverlayContext(FundingOverlayRequest(null, "intent", FundingDirection.IN, null))
        contexts.put(sheet)

        val outcome = async { overlay.presentProviderFrame(ProductId.fromStoredValue("ramp.dot"), "intent", "/pay") }
        runCurrent()
        coVerify(exactly = 0) { router.openFundingProviderFrame(any()) }

        sheet.markClosed()
        runCurrent()
        coVerify { router.openFundingProviderFrame("intent") }

        val frame = requireNotNull(frames.get("intent"))
        assertFalse(frame.showsSentFunds)
        frame.answer(ProviderFrameOutcome.CLOSED)
        assertEquals(ProviderFrameOutcome.CLOSED, outcome.await())
    }

    @Test
    fun `a bank transfer's provider screen offers to say the funds were sent`() = runTest {
        coEvery { runtime.session("intent") } returns session(FundingRail.BANK, FundingStage.Open)

        val outcome = async { overlay.presentProviderFrame(ProductId.fromStoredValue("ramp.dot"), "intent", "pay") }
        runCurrent()

        val frame = requireNotNull(frames.get("intent"))
        assertTrue(frame.showsSentFunds)
        frame.answer(ProviderFrameOutcome.DISMISSED)
        assertEquals(ProviderFrameOutcome.DISMISSED, outcome.await())
    }

    @Test
    fun `the overlay answers what its sheet answered`() = runTest {
        val outcome = async { overlay.present(FundingOverlayRequest(null, "intent", FundingDirection.IN, null)) }
        runCurrent()

        requireNotNull(contexts.get("intent")).answer(FundingOverlayOutcome.STARTED)

        assertEquals(FundingOverlayOutcome.STARTED, outcome.await())
    }

    @Test
    fun `the provider screen has done its part once a step past started is reached`() = runTest {
        val interactor = FundingFlowInteractor(mockk(), mockk<FundingBalance>(), runtime, mockk())
        coEvery { runtime.session("intent") } returns session(FundingRail.CARD, FundingStage.Open)
        coEvery { runtime.progress("intent") } returnsMany listOf(
            progress(FundingProgressStep(FundingStep.STARTED, 1uL), FundingProgressStep(FundingStep.PAYMENT, null)),
            progress(FundingProgressStep(FundingStep.STARTED, 1uL), FundingProgressStep(FundingStep.PAYMENT, 2uL)),
        )

        assertEquals(listOf(false, true), listOf(interactor.paymentMoved("intent"), interactor.paymentMoved("intent")))
    }

    private fun progress(vararg steps: FundingProgressStep) = FundingProgress(
        steps = steps.toList(),
        failedAtMs = null,
        transactionId = null,
        reference = null,
        deposit = null,
        mismatch = null,
        retrying = false,
        payout = null,
    )

    private fun session(rail: FundingRail, stage: FundingStage) = FundingSession(
        intent = "intent",
        ownerProductId = null,
        direction = CoreFundingDirection.IN,
        amount = "50000000",
        stage = stage,
        openedAtMs = 0uL,
        deadlineMs = 0uL,
        acknowledged = false,
        providerId = "ramp.dot",
        cancelRequested = false,
        updates = emptyList(),
        choice = FundingChoice(
            quote = FundingQuote("q", "5100", "50000000", "90", "10", null, null),
            rail = rail,
            asset = "EUR",
        ),
        saved = null,
    )
}
