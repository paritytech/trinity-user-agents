package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameTarget
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.FundingBand
import uniffi.truapi.FundingBookEntry
import uniffi.truapi.FundingCancelAvailability
import uniffi.truapi.FundingChoice
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingStatus
import uniffi.truapi.ProductId
import uniffi.truapi.FundingDirection as CoreFundingDirection

class AppFundingOverlayTest {
    private val ramp = ProductId.parse("ramp.dot").getOrThrow()
    private val contexts = FundingOverlayContexts()
    private val frames = FundingFrameContexts()
    private val sessions = MutableStateFlow<Map<String, FundingBookEntry>>(emptyMap())
    private val runtime = mockk<FundingRuntime> {
        every { sessionChanges() } returns MutableSharedFlow()
        every { sessions } returns this@AppFundingOverlayTest.sessions
        coEvery { sessionOpen(any()) } answers { sessions.value[firstArg()]?.session?.stage is FundingStage.Open }
    }

    // A sheet that appears marks itself shown, as the real one does once it finds its context.
    private val router = mockk<ProductsRouter>(relaxed = true) {
        coEvery { openFundingOverlay(any()) } answers { contexts.get(firstArg())?.markShown() }
    }
    private val overlay = AppFundingOverlay(contexts, frames, runtime, router)

    @Test
    fun `a card's provider screen opens over its sheet`() = runTest {
        push(session(FundingRail.CARD, FundingStage.Open))
        contexts.put(FundingOverlayContext(FundingOverlayRequest(null, "intent", FundingDirection.IN, null), FundingOverlayMode.New))

        val outcome = async { overlay.presentProviderFrame(ramp, "intent", ProviderFrameTarget.App("/pay")) }
        runCurrent()
        coVerify { router.openFundingProviderFrame("intent") }

        val frame = requireNotNull(frames.get("intent"))
        frame.answer(ProviderFrameOutcome.Closed)
        assertEquals(ProviderFrameOutcome.Closed, outcome.await())
    }

    // A card payment the user made: the provider is done with its screen, which gives way to the session's sheet.
    @Test
    fun `a provider closing its screen answers it closed and shows the session's sheet in its place`() = runTest {
        push(session(FundingRail.CARD, FundingStage.Open))

        val outcome = async { overlay.presentProviderFrame(ramp, "intent", ProviderFrameTarget.App("/pay")) }
        runCurrent()
        overlay.closeProviderFrame(ramp, "intent")

        assertEquals(ProviderFrameOutcome.Closed, outcome.await())
        coVerifyOrder {
            router.closeFundingProviderFrame("intent")
            router.openFundingOverlay("intent")
        }
        assertEquals(FundingOverlayMode.Resumed, contexts.get("intent")?.mode)
        assertEquals(null, frames.get("intent"))
    }

    @Test
    fun `a provider closing its screen over the open sheet leaves the sheet as it is`() = runTest {
        push(session(FundingRail.CARD, FundingStage.Open))
        val sheet = FundingOverlayContext(FundingOverlayRequest(null, "intent", FundingDirection.IN, null), FundingOverlayMode.New)
        contexts.put(sheet)

        val outcome = async { overlay.presentProviderFrame(ramp, "intent", ProviderFrameTarget.App("/pay")) }
        runCurrent()
        overlay.closeProviderFrame(ramp, "intent")

        assertEquals(ProviderFrameOutcome.Closed, outcome.await())
        assertSame(sheet, contexts.get("intent"))
        coVerify(exactly = 0) { router.openFundingOverlay(any()) }
    }

    @Test
    fun `closing a screen that is not open, or another provider's, does nothing`() = runTest {
        overlay.closeProviderFrame(ramp, "intent")
        push(session(FundingRail.CARD, FundingStage.Open))

        val outcome = async { overlay.presentProviderFrame(ramp, "intent", ProviderFrameTarget.App("/pay")) }
        runCurrent()
        overlay.closeProviderFrame(ProductId.parse("other.dot").getOrThrow(), "intent")
        runCurrent()
        assertFalse(outcome.isCompleted)

        requireNotNull(frames.get("intent")).answer(ProviderFrameOutcome.Dismissed)
        assertEquals(ProviderFrameOutcome.Dismissed, outcome.await())
        coVerify(exactly = 0) { router.openFundingOverlay(any()) }
    }

    @Test
    fun `the overlay answers what its sheet answered`() = runTest {
        val outcome = async { overlay.present(FundingOverlayRequest(null, "intent", FundingDirection.IN, null)) }
        runCurrent()

        requireNotNull(contexts.get("intent")).answer(FundingOverlayOutcome.STARTED)

        assertEquals(FundingOverlayOutcome.STARTED, outcome.await())
    }

    @Test
    fun `a sheet that never appears is answered dismissed and leaves the session free to open again`() = runTest {
        coEvery { router.openFundingOverlay(any()) } returns Unit

        val outcome = overlay.present(FundingOverlayRequest(null, "intent", FundingDirection.IN, null))

        assertEquals(FundingOverlayOutcome.DISMISSED, outcome)
        assertNull(contexts.get("intent"))
        coVerify { router.closeFundingOverlay("intent") }
    }

    @Test
    fun `a resumed sheet that never appears can be opened again`() = runTest {
        push(session(FundingRail.CARD, FundingStage.Open))
        coEvery { router.openFundingOverlay(any()) } returns Unit

        assertTrue(overlay.reopen("intent"))
        assertNull(contexts.get("intent"))

        coEvery { router.openFundingOverlay(any()) } answers { contexts.get(firstArg())?.markShown() }
        assertTrue(overlay.reopen("intent"))
        assertEquals(FundingOverlayMode.Resumed, contexts.get("intent")?.mode)
        coVerify(exactly = 2) { router.openFundingOverlay("intent") }
    }

    // The core pushes an ended session until it is acknowledged; it shows on its outcome, not as a sheet in flight.
    @Test
    fun `an ended session the core still holds does not reopen as a sheet`() = runTest {
        push(session(FundingRail.CARD, FundingStage.Delivered("50000000", settledAtMs = 1uL)))

        assertFalse(overlay.reopen("intent"))
        assertNull(contexts.get("intent"))
        coVerify(exactly = 0) { router.openFundingOverlay(any()) }
    }

    @Test
    fun `a sheet the core stopped waiting on is marked closed`() = runTest {
        val outcome = async { overlay.present(FundingOverlayRequest(null, "intent", FundingDirection.IN, null)) }
        runCurrent()
        val sheet = requireNotNull(contexts.get("intent"))

        outcome.cancel()
        runCurrent()

        assertTrue(sheet.isClosed)
    }

    private fun push(session: FundingSession) {
        sessions.value = mapOf(session.intent to FundingBookEntry(session, progress()))
    }

    private fun progress() = FundingProgress(
        steps = emptyList(),
        band = FundingBand.AWAITING_PAYMENT,
        lastHeardAtMs = null,
        detail = null,
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
        moneyMoved = false,
        awaitsUserTransfer = false,
        cancel = FundingCancelAvailability.AVAILABLE,
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
        providerId = ramp,
        assignedAtMs = 0uL,
        cancel = null,
        updates = emptyList(),
        started = emptyList(),
        choice = FundingChoice(
            quote = FundingQuote("q", "5100", "50000000", "90", "10", null, null, 2u),
            rail = rail,
            asset = "EUR",
            network = null,
        ),
        refund = null,
        detail = null,
    )
}
