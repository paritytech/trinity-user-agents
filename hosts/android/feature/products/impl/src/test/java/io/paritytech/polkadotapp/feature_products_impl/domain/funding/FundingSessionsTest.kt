package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
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
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingPresentOutcome
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSelection
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.FundingStage
import uniffi.truapi.ProductId

class FundingSessionsTest {
    private val ramp = ProductId.parse("ramp.dot").getOrThrow()
    private val other = ProductId.parse("other.dot").getOrThrow()

    // What the core waits on outlives each test; foreground, so advanceUntilIdle runs it, and outside the test's job, so the test does not wait for it.
    private val detached = Job()
    private val core = FakeFundingCore().apply {
        sessions["intent"] = FakeFundingCore.session("intent")
        options["intent"] = FakeFundingCore.options(ramp, other)
    }

    private lateinit var quotes: RealFundingQuotes

    private fun TestScope.service(): RealFundingSessions {
        val calls = FundingCoreCalls(this + detached, testDispatchers())
        quotes = RealFundingQuotes(core, calls)
        return RealFundingSessions(core, quotes, calls, testDispatchers())
    }

    /** Presents session "intent" as the core does, with its screens up so the presentation holds. */
    private fun TestScope.presented(service: RealFundingSessions) = async(detached) { service.present(INTENT, FundingDirection.IN, null) }
        .also {
            (this + detached).launch { service.shown(INTENT) }
            service.entryState(INTENT).launchIn(this + detached)
        }

    @Test
    fun `a session's state comes first, with the options the core offers while it is set up`() = runTest {
        val service = service()
        presented(service)
        advanceUntilIdle()

        val entry = service.entryState(INTENT).first()

        assertEquals(FundingPhase.SELECTING, entry.phase)
        assertEquals(listOf(ramp, other), entry.options?.candidates?.map { it.providerId })
        assertTrue(entry.isPresented)
    }

    @Test
    fun `only the latest ask's list is shown`() = runTest {
        val service = service()
        presented(service)
        advanceUntilIdle()

        quotes.quote(100.intoBalance(), INTENT)
        quotes.quote(200.intoBalance(), INTENT)
        advanceUntilIdle()
        core.quotes["intent"] = FakeFundingCore.list(1uL, "100", ramp to FundingQuoteState.Quoted(FakeFundingCore.quote("q-1")))
        quotes.onQuotesChanged(INTENT)
        advanceUntilIdle()
        assertNull(service.entryState(INTENT).first().quotes)

        core.quotes["intent"] = FakeFundingCore.list(2uL, "200", ramp to FundingQuoteState.Quoted(FakeFundingCore.quote("q-2")))
        quotes.onQuotesChanged(INTENT)
        advanceUntilIdle()
        val entry = service.entryState(INTENT).first()

        assertEquals(2uL, entry.quotes?.requestId)
        assertEquals(200.intoBalance(), entry.askedAmount)
    }

    @Test
    fun `the selection reaches the core before the ask made after it`() = runTest {
        val service = service()
        presented(service)

        service.select(FundingSelection(FundingRail.CARD, null, null, "DE", "EUR"), INTENT)
        quotes.quote(100.intoBalance(), INTENT)
        advanceUntilIdle()

        assertEquals(listOf("select CARD", "quote 100"), core.calls)
    }

    @Test
    fun `starting on a provider's quote hands it the session and answers the core started`() = runTest {
        val service = service()
        val outcome = presented(service)
        quotes.quote(100.intoBalance(), INTENT)
        advanceUntilIdle()
        core.quotes["intent"] = FakeFundingCore.list(1uL, "100", ramp to FundingQuoteState.Quoted(FakeFundingCore.quote("q-1")))
        quotes.onQuotesChanged(INTENT)
        advanceUntilIdle()

        val started = service.start(INTENT, ramp)
        advanceUntilIdle()

        assertTrue(started.isSuccess)
        assertTrue("selectProvider ramp.dot q-1" in core.calls)
        assertEquals(FundingPresentOutcome.STARTED, outcome.await())
        assertEquals(FundingPhase.RUNNING, service.entryState(INTENT).first().phase)
    }

    @Test
    fun `a provider without a quote on screen cannot be started on`() = runTest {
        val service = service()
        presented(service)
        advanceUntilIdle()

        val started = service.start(INTENT, ramp)

        assertTrue(started.exceptionOrNull() is FundingStartError.NoQuote)
    }

    @Test
    fun `a provider that refuses the session says why`() = runTest {
        val service = service()
        presented(service)
        core.providerTakes = false
        quotes.quote(100.intoBalance(), INTENT)
        advanceUntilIdle()
        core.quotes["intent"] = FakeFundingCore.list(1uL, "100", ramp to FundingQuoteState.Quoted(FakeFundingCore.quote("q-1")))
        quotes.onQuotesChanged(INTENT)
        advanceUntilIdle()

        val started = service.start(INTENT, ramp)

        assertTrue(started.exceptionOrNull() is FundingStartError.NotSelected)
    }

    @Test
    fun `starting on the best quote arms the core on the latest ask, and the session's start answers the core`() = runTest {
        val service = service()
        val outcome = presented(service)
        quotes.quote(100.intoBalance(), INTENT)
        advanceUntilIdle()

        service.start(INTENT, null)
        advanceUntilIdle()
        assertTrue("startWhenQuoted 1" in core.calls)

        core.sessions["intent"] = FakeFundingCore.session("intent", providerId = ramp)
        service.onSessionChanged(INTENT)
        advanceUntilIdle()

        assertEquals(FundingPresentOutcome.STARTED, outcome.await())
    }

    @Test
    fun `an ask made after arming the start is started on too`() = runTest {
        val service = service()
        presented(service)
        advanceUntilIdle()

        service.start(INTENT, null)
        quotes.quote(100.intoBalance(), INTENT)
        advanceUntilIdle()

        assertTrue("startWhenQuoted 1" in core.calls)
    }

    @Test
    fun `leaving the start screen disarms the core`() = runTest {
        val service = service()
        presented(service)
        quotes.quote(100.intoBalance(), INTENT)
        service.start(INTENT, null)
        advanceUntilIdle()

        quotes.stopStarting(INTENT)
        advanceUntilIdle()

        assertEquals("cancelAutoStart", core.calls.last())
    }

    @Test
    fun `leaving a session that never started dismisses it and ends whatever follows it`() = runTest {
        val service = service()
        val outcome = presented(service)
        advanceUntilIdle()
        val followed = async { service.entryState(INTENT).toList() }
        advanceUntilIdle()

        service.leave(INTENT)
        advanceUntilIdle()

        assertEquals(FundingPresentOutcome.DISMISSED, outcome.await())
        assertEquals(1, followed.await().size)
    }

    // Navigation can fail silently; the core must not wait on screens that never came.
    @Test
    fun `a session whose screens do not come up within ten seconds is taken as dismissed`() = runTest {
        val service = service()
        val outcome = async(detached) { service.present(INTENT, FundingDirection.IN, null) }
        service.entryState(INTENT).launchIn(this + detached)

        advanceTimeBy(9_999)
        assertFalse(outcome.isCompleted)
        advanceTimeBy(2)

        assertEquals(FundingPresentOutcome.DISMISSED, outcome.await())
    }

    @Test
    fun `the presentation is asked for through the events`() = runTest {
        val service = service()
        val requested = async { service.presentationRequests.first() }
        presented(service)
        advanceUntilIdle()

        assertEquals(INTENT, requested.await().id)
    }

    @Test
    fun `cancelling a session that never started goes to the core too`() = runTest {
        core.cancelOutcome = CancelOutcome.CANCELLED
        val service = service()
        presented(service)
        advanceUntilIdle()

        val cancelled = service.cancel(INTENT)

        assertEquals(CancelOutcome.CANCELLED, cancelled.getOrNull())
        assertTrue("cancel" in core.calls)
        assertFalse("cancelAutoStart" in core.calls)
    }

    @Test
    fun `cancelling a session in flight asks the core`() = runTest {
        core.sessions["intent"] = FakeFundingCore.session("intent", providerId = ramp)
        val service = service()

        val cancelled = service.cancel(INTENT)

        assertEquals(CancelOutcome.REQUESTED, cancelled.getOrNull())
        assertTrue("cancel" in core.calls)
    }

    @Test
    fun `an ended session ends what follows it after its last state`() = runTest {
        core.sessions["intent"] = FakeFundingCore.session("intent", providerId = ramp)
        val service = service()
        val followed = async { service.entryState(INTENT).toList() }
        advanceUntilIdle()

        core.sessions["intent"] = FakeFundingCore.session("intent", providerId = ramp, stage = FundingStage.Failed(FundingFailure.Expired, 2uL))
        service.onSessionChanged(INTENT)
        advanceUntilIdle()

        val states = followed.await()
        assertEquals(listOf(FundingPhase.RUNNING, FundingPhase.ENDED), states.map { it.phase })
        assertEquals(FundingFailure.Expired, states.last().failure)
    }

    @Test
    fun `a session restored without the core holding it is gone`() = runTest {
        core.sessions.clear()
        val service = service()

        assertEquals(emptyList<FundingEntry>(), service.entryState(INTENT).toList())
    }

    @Test
    fun `the list holds every session in flight and every ended one, newest first, each once`() = runTest {
        core.sessions["intent"] = FakeFundingCore.session("intent", providerId = ramp, openedAtMs = 2uL)
        core.history += summary("intent", openedAtMs = 2uL)
        core.history += summary("older", openedAtMs = 1uL)
        core.history += summary("newest", openedAtMs = 3uL)
        val service = service()

        val entries = service.entries().first()

        assertEquals(listOf("newest", "intent", "older"), entries.map { it.id.value })
    }

    // The core's history leaves out a top-up cancelled before anything settled or was refunded.
    @Test
    fun `an ended session's row reads its summary from the history`() = runTest {
        val failed = FundingStage.Failed(FundingFailure.Cancelled, 2uL)
        core.sessions["intent"] = FakeFundingCore.session("intent", providerId = ramp, stage = failed)
        core.summaries["intent"] = summary("intent", openedAtMs = 1uL)
        val service = service()

        val entry = service.entries().first().single()

        assertEquals(FundingPhase.ENDED, entry.phase)
        assertNull(entry.summary)
    }

    @Test
    fun `acknowledging an ended session lists the sessions again`() = runTest {
        val service = service()
        val lists = mutableListOf<List<FundingEntry>>()
        service.entries().onEach { lists += it }.launchIn(this + detached)
        advanceUntilIdle()

        service.acknowledge(INTENT)
        advanceUntilIdle()

        assertTrue("acknowledge intent" in core.calls)
        assertEquals(2, lists.size)
    }

    @Test
    fun `a quote request that does not reach the core keeps the ask, marked failed`() = runTest {
        val service = service()
        presented(service)
        core.quoteRequestFails = true

        quotes.quote(100.intoBalance(), INTENT)
        advanceUntilIdle()
        val entry = service.entryState(INTENT).first()

        assertEquals(100.intoBalance(), entry.askedAmount)
        assertTrue(entry.quotesFailed)
    }

    @Test
    fun `a quote list that cannot be read keeps the quotes on screen, marked failed, until a read succeeds`() = runTest {
        val service = service()
        presented(service)
        quotes.quote(100.intoBalance(), INTENT)
        advanceUntilIdle()
        core.quotes["intent"] = FakeFundingCore.list(1uL, "100", ramp to FundingQuoteState.Quoted(FakeFundingCore.quote("q-1")))
        quotes.onQuotesChanged(INTENT)
        advanceUntilIdle()

        core.quotesReadFails = true
        quotes.onQuotesChanged(INTENT)
        advanceUntilIdle()
        val failed = service.entryState(INTENT).first()
        core.quotesReadFails = false
        quotes.onQuotesChanged(INTENT)
        advanceUntilIdle()
        val read = service.entryState(INTENT).first()

        assertEquals(1uL, failed.quotes?.requestId)
        assertTrue(failed.quotesFailed)
        assertFalse(read.quotesFailed)
    }

    private fun summary(intent: String, openedAtMs: ULong) = mockk<FundingSessionSummary>(relaxed = true) {
        every { this@mockk.intent } returns intent
        every { this@mockk.openedAtMs } returns openedAtMs
        every { direction } returns FundingDirection.IN
        every { stage } returns FundingStage.Failed(FundingFailure.Expired, openedAtMs)
    }

    private companion object {
        val INTENT = FundingIntentId("intent")
    }
}
