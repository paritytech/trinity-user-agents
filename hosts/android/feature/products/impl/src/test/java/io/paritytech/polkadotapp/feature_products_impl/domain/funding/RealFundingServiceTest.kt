package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.plus
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.CancelOutcome
import uniffi.truapi.FundingChangeKind
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingFrameTarget
import uniffi.truapi.FundingPresentOutcome
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSelection
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.FundingStage
import uniffi.truapi.ProductId

class RealFundingServiceTest {
    private val ramp = ProductId.parse("ramp.dot").getOrThrow()
    private val other = ProductId.parse("other.dot").getOrThrow()

    // What the core waits on outlives each test; foreground, so advanceUntilIdle runs it, and outside the test's job, so the test does not wait for it.
    private val detached = Job()
    private val core = FakeFundingCore().apply {
        sessions["intent"] = FakeFundingCore.session("intent")
        options["intent"] = FakeFundingCore.options(ramp, other)
    }

    private fun TestScope.service() = RealFundingService(core, testDispatchers())

    /** Presents session "intent" as the core does, with something following it so the presentation holds. */
    private fun TestScope.presented(service: RealFundingService) = async(detached) { service.present("intent", FundingDirection.IN, null) }
        .also { service.entryState("intent").launchIn(this + detached) }

    @Test
    fun `a session's state comes first, with the options the core offers while it is set up`() = runTest {
        val service = service()
        presented(service)
        advanceUntilIdle()

        val entry = service.entryState("intent").first()

        assertEquals(FundingPhase.SELECTING, entry.phase)
        assertEquals(listOf(ramp, other), entry.options?.candidates?.map { it.providerId })
        assertTrue(entry.isPresented)
    }

    @Test
    fun `only the latest ask's list is shown`() = runTest {
        val service = service()
        presented(service)
        advanceUntilIdle()

        service.quote("100", "intent")
        service.quote("200", "intent")
        advanceUntilIdle()
        core.quotes["intent"] = FakeFundingCore.list(1uL, "100", ramp to FundingQuoteState.Quoted(FakeFundingCore.quote("q-1")))
        service.changed("intent", FundingChangeKind.QUOTES)
        advanceUntilIdle()
        assertNull(service.entryState("intent").first().quotes)

        core.quotes["intent"] = FakeFundingCore.list(2uL, "200", ramp to FundingQuoteState.Quoted(FakeFundingCore.quote("q-2")))
        service.changed("intent", FundingChangeKind.QUOTES)
        advanceUntilIdle()
        val entry = service.entryState("intent").first()

        assertEquals(2uL, entry.quotes?.requestId)
        assertFalse(entry.isQuoting)
    }

    @Test
    fun `the selection reaches the core before the ask made after it`() = runTest {
        val service = service()
        presented(service)

        service.select(FundingSelection(FundingRail.CARD, null, null, "DE", "EUR"), "intent")
        service.quote("100", "intent")
        advanceUntilIdle()

        assertEquals(listOf("select CARD", "quote 100"), core.calls)
    }

    // A provider asked again keeps its last price greyed until it answers.
    @Test
    fun `asking the same amount again keeps the quotes on screen until their providers answer`() = runTest {
        val service = service()
        presented(service)
        service.quote("100", "intent")
        advanceUntilIdle()
        val quoted = FakeFundingCore.quote("q-1")
        core.quotes["intent"] = FakeFundingCore.list(1uL, "100", ramp to FundingQuoteState.Quoted(quoted))
        service.changed("intent", FundingChangeKind.QUOTES)
        advanceUntilIdle()

        service.quote("100", "intent")
        advanceUntilIdle()
        core.quotes["intent"] = FakeFundingCore.list(2uL, "100", ramp to FundingQuoteState.Pending, complete = false)
        service.changed("intent", FundingChangeKind.QUOTES)
        advanceUntilIdle()

        assertEquals(mapOf(ramp to quoted), service.entryState("intent").first().staleQuotes)
    }

    @Test
    fun `starting on a provider's quote hands it the session and answers the core started`() = runTest {
        val service = service()
        val outcome = presented(service)
        service.quote("100", "intent")
        advanceUntilIdle()
        core.quotes["intent"] = FakeFundingCore.list(1uL, "100", ramp to FundingQuoteState.Quoted(FakeFundingCore.quote("q-1")))
        service.changed("intent", FundingChangeKind.QUOTES)
        advanceUntilIdle()

        val started = service.start("intent", ramp)
        advanceUntilIdle()

        assertTrue(started.isSuccess)
        assertTrue("selectProvider ramp.dot q-1" in core.calls)
        assertEquals(FundingPresentOutcome.STARTED, outcome.await())
        assertEquals(FundingPhase.RUNNING, service.entryState("intent").first().phase)
    }

    @Test
    fun `a provider without a quote on screen cannot be started on`() = runTest {
        val service = service()
        presented(service)
        advanceUntilIdle()

        val started = service.start("intent", ramp)

        assertTrue(started.exceptionOrNull() is FundingStartError.NoQuote)
    }

    @Test
    fun `a provider that refuses the session says why`() = runTest {
        val service = service()
        presented(service)
        core.providerTakes = false
        service.quote("100", "intent")
        advanceUntilIdle()
        core.quotes["intent"] = FakeFundingCore.list(1uL, "100", ramp to FundingQuoteState.Quoted(FakeFundingCore.quote("q-1")))
        service.changed("intent", FundingChangeKind.QUOTES)
        advanceUntilIdle()

        val started = service.start("intent", ramp)

        assertTrue(started.exceptionOrNull() is FundingStartError.NotSelected)
    }

    @Test
    fun `starting on the best quote arms the core on the latest ask, and the session's start answers the core`() = runTest {
        val service = service()
        val outcome = presented(service)
        service.quote("100", "intent")
        advanceUntilIdle()

        service.start("intent", null)
        advanceUntilIdle()
        assertTrue("startWhenQuoted 1" in core.calls)

        core.sessions["intent"] = FakeFundingCore.session("intent", providerId = ramp)
        service.changed("intent", FundingChangeKind.SESSION)
        advanceUntilIdle()

        assertEquals(FundingPresentOutcome.STARTED, outcome.await())
    }

    @Test
    fun `an ask made after arming the start is started on too`() = runTest {
        val service = service()
        presented(service)
        advanceUntilIdle()

        service.start("intent", null)
        service.quote("100", "intent")
        advanceUntilIdle()

        assertTrue("startWhenQuoted 1" in core.calls)
    }

    @Test
    fun `leaving the start screen disarms the core`() = runTest {
        val service = service()
        presented(service)
        service.quote("100", "intent")
        service.start("intent", null)
        advanceUntilIdle()

        service.stopStarting("intent")
        advanceUntilIdle()

        assertEquals("cancelAutoStart", core.calls.last())
    }

    @Test
    fun `leaving a session that never started dismisses it and ends whatever follows it`() = runTest {
        val service = service()
        val outcome = presented(service)
        advanceUntilIdle()
        val followed = async { service.entryState("intent").toList() }
        advanceUntilIdle()

        service.leave("intent")
        advanceUntilIdle()

        assertEquals(FundingPresentOutcome.DISMISSED, outcome.await())
        assertEquals(1, followed.await().size)
    }

    // Navigation can fail silently; the core must not wait on screens that never came.
    @Test
    fun `a session nobody follows within ten seconds is taken as dismissed`() = runTest {
        val service = service()
        val outcome = async(detached) { service.present("intent", FundingDirection.IN, null) }

        advanceTimeBy(9_999)
        assertFalse(outcome.isCompleted)
        advanceTimeBy(2)

        assertEquals(FundingPresentOutcome.DISMISSED, outcome.await())
    }

    @Test
    fun `the presentation is asked for through the events`() = runTest {
        val service = service()
        val event = async { service.events.first() }
        presented(service)
        advanceUntilIdle()

        val requested = event.await() as FundingEvent.PresentationRequested
        assertEquals("intent", requested.entry.id)
    }

    @Test
    fun `cancelling a session that never started drops it without the core`() = runTest {
        val service = service()
        val outcome = presented(service)
        advanceUntilIdle()

        val cancelled = service.cancel("intent")

        assertEquals(CancelOutcome.ENDED, cancelled.getOrNull())
        assertEquals(FundingPresentOutcome.DISMISSED, outcome.await())
        assertFalse("cancel" in core.calls)
    }

    @Test
    fun `cancelling a session in flight asks the core`() = runTest {
        core.sessions["intent"] = FakeFundingCore.session("intent", providerId = ramp)
        val service = service()

        val cancelled = service.cancel("intent")

        assertEquals(CancelOutcome.REQUESTED, cancelled.getOrNull())
        assertTrue("cancel" in core.calls)
    }

    @Test
    fun `an ended session ends what follows it after its last state`() = runTest {
        core.sessions["intent"] = FakeFundingCore.session("intent", providerId = ramp)
        val service = service()
        val followed = async { service.entryState("intent").toList() }
        advanceUntilIdle()

        core.sessions["intent"] = FakeFundingCore.session("intent", providerId = ramp, stage = FundingStage.Failed(FundingFailure.Expired, 2uL))
        service.changed("intent", FundingChangeKind.SESSION)
        advanceUntilIdle()

        val states = followed.await()
        assertEquals(listOf(FundingPhase.RUNNING, FundingPhase.ENDED), states.map { it.phase })
        assertEquals(FundingFailure.Expired, states.last().failure)
    }

    @Test
    fun `a session restored without the core holding it is gone`() = runTest {
        core.sessions.clear()
        val service = service()

        assertEquals(emptyList<FundingEntry>(), service.entryState("intent").toList())
    }

    @Test
    fun `a provider's screen is asked for, and answered as the app took it down`() = runTest {
        val service = service()
        val target = FundingFrameTarget.App("/kyc")
        val outcome = async(detached) { service.presentFrame(ramp, "intent", target) }
        val request = (service.events.first() as FundingEvent.FrameRequested).request

        service.answerFrame(request, FundingFrameOutcome.Dismissed)

        assertEquals(FundingFrameTarget.App("/kyc"), request.target)
        assertEquals(FundingFrameOutcome.Dismissed, outcome.await())
    }

    @Test
    fun `the provider closing its screens answers them closed and takes them down`() = runTest {
        val service = service()
        val outcome = async(detached) { service.presentFrame(ramp, "intent", FundingFrameTarget.App("/kyc")) }
        service.events.first()

        service.closeFrames(ramp, "intent")

        assertEquals(FundingFrameOutcome.Closed, outcome.await())
        assertEquals(FundingEvent.FrameClosed("intent"), service.events.first())
    }

    @Test
    fun `the list holds every session in flight and every ended one, newest first, each once`() = runTest {
        core.sessions["intent"] = FakeFundingCore.session("intent", providerId = ramp, openedAtMs = 2uL)
        core.history += summary("intent", openedAtMs = 2uL)
        core.history += summary("older", openedAtMs = 1uL)
        core.history += summary("newest", openedAtMs = 3uL)
        val service = service()

        val entries = service.entries().first()

        assertEquals(listOf("newest", "intent", "older"), entries.map { it.id })
    }

    @Test
    fun `acknowledging an ended session lists the sessions again`() = runTest {
        val service = service()
        val lists = mutableListOf<List<FundingEntry>>()
        service.entries().onEach { lists += it }.launchIn(this + detached)
        advanceUntilIdle()

        service.acknowledge("intent")
        advanceUntilIdle()

        assertTrue("acknowledge intent" in core.calls)
        assertEquals(2, lists.size)
    }

    @Test
    fun `a crashing provider of a session it has seen is reported, any other product is not`() = runTest {
        val service = service()
        presented(service)
        advanceUntilIdle()
        val event = async { service.events.first { it is FundingEvent.ProviderNotWorking } }

        assertTrue(service.workerCrashing(ramp))
        assertFalse(service.workerCrashing(ProductId.parse("chat.dot").getOrThrow()))
        assertEquals(FundingEvent.ProviderNotWorking(ramp), event.await())
    }

    private fun summary(intent: String, openedAtMs: ULong) = mockk<FundingSessionSummary>(relaxed = true) {
        every { this@mockk.intent } returns intent
        every { this@mockk.openedAtMs } returns openedAtMs
        every { direction } returns FundingDirection.IN
        every { stage } returns FundingStage.Failed(FundingFailure.Expired, openedAtMs)
    }
}
