package io.paritytech.polkadotapp.feature_products_impl.domain.funding.history

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_impl.data.repository.FundingHistoryRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingPayout
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingStatus
import kotlin.time.Clock

class FundingActivityCenterTest {
    private val runtime = mockk<FundingRuntime> {
        every { sessionChanges() } returns emptyFlow()
        coEvery { acknowledge(any()) } returns Result.success(true)
    }
    private val saved = mutableListOf<FundingRecord>()
    private val history = mockk<FundingHistoryRepository> {
        val record = slot<FundingRecord>()
        coEvery { save(capture(record)) } answers { saved += record.captured; Result.success(Unit) }
        coEvery { records() } answers { Result.success(saved.reversed().distinctBy { it.intent }) }
    }
    private val center = FundingActivityCenter(runtime, history)

    @Test
    fun `an ended top-up is written and then acknowledged`() = runBlocking {
        serve(session("in", FundingDirection.IN, FundingStage.Delivered("50", settledAtMs = 1uL)) to progress(FundingStatus.ToppedUp))

        val activity = center.observe().first()

        assertEquals(listOf("in"), saved.map { it.intent })
        coVerify { runtime.acknowledge("in") }
        assertEquals(listOf(FundingActivityStatus.ToppedUp), activity.history.map { it.status })
    }

    // The core keeps a released withdrawal until its payout is reported, so the host need not hold it back.
    @Test
    fun `a released withdrawal is kept and acknowledged, and stays in progress until its payout`() = runBlocking {
        serve(session("out", FundingDirection.OUT, FundingStage.Released("50", settledAtMs = 1uL)) to progress(FundingStatus.PreparingPayout))

        val activity = center.observe().first()

        coVerify { runtime.acknowledge("out") }
        assertEquals(listOf(FundingActivityStep.PreparingPayout), activity.inFlight.map { (it.status as FundingActivityStatus.InProgress).step })
        assertEquals(emptyList<String>(), activity.history.map { it.id })
    }

    @Test
    fun `an acknowledged session is not written again`() = runBlocking {
        serve(session("in", FundingDirection.IN, FundingStage.Delivered("50", settledAtMs = 1uL), acknowledged = true) to progress(FundingStatus.ToppedUp))

        center.observe().first()

        assertEquals(emptyList<String>(), saved.map { it.intent })
        coVerify(exactly = 0) { runtime.acknowledge(any()) }
    }

    // A late payout or refund report hands the session back unacknowledged, and its stored row has to follow.
    @Test
    fun `a failed payout's refund rewrites the stored row with what came back`() = runBlocking {
        val released = FundingStage.Released("50", settledAtMs = 1uL)
        serve(session("out", FundingDirection.OUT, released) to progress(FundingStatus.PayoutFailed(refunded = null), FundingPayout.Failed("rejected")))
        center.observe().first()

        serve(session("out", FundingDirection.OUT, released) to progress(FundingStatus.PayoutFailed(refunded = "45"), FundingPayout.Failed("rejected"), refunded = "45"))
        val activity = center.observe().first()

        assertEquals(listOf(null, 45.intoBalance()), saved.map { it.refunded })
        assertEquals(listOf(FundingActivityStatus.PayoutFailed(45.intoBalance())), activity.history.map { it.status })
    }

    @Test
    fun `the line in flight is the core's status`() = runBlocking {
        serve(
            session("longer", FundingDirection.IN, FundingStage.Open) to progress(FundingStatus.TakingLonger),
            session("retrying", FundingDirection.IN, FundingStage.Open) to progress(FundingStatus.Retrying),
            session("payout", FundingDirection.OUT, FundingStage.Open) to progress(FundingStatus.RetryingPayout),
            session("failed", FundingDirection.IN, FundingStage.Failed(FundingFailure.Declined, settledAtMs = 1uL)) to
                progress(FundingStatus.Failed(FundingFailure.Declined)),
        )

        val activity = center.observe().first()

        assertEquals(
            mapOf(
                "longer" to FundingActivityStep.TakingLonger,
                "retrying" to FundingActivityStep.Retrying,
                "payout" to FundingActivityStep.RetryingPayout,
            ),
            activity.inFlight.associate { it.id to (it.status as FundingActivityStatus.InProgress).step },
        )
        assertEquals(listOf(FundingActivityStatus.Failed(FundingFailureReason.Declined)), activity.history.map { it.status })
    }

    // No status change announces a session turning slow, so the list wakes itself at the time the core gave.
    @Test
    fun `the list is read again once a session turns taking longer`() = runBlocking {
        val longerAt = Clock.System.now().toEpochMilliseconds().toULong() + 50uL
        serve(session("in", FundingDirection.IN, FundingStage.Open) to progress(FundingStatus.WaitingForPayment, longerAtMs = longerAt))

        val reads = withTimeout(2_000) { center.observe().take(2).toList() }

        assertEquals(2, reads.size)
    }

    private fun serve(vararg entries: Pair<FundingSession, FundingProgress>) {
        coEvery { runtime.sessions() } returns entries.map { it.first }
        entries.forEach { (session, progress) -> coEvery { runtime.progress(session.intent) } returns progress }
    }

    private fun progress(
        status: FundingStatus,
        payout: FundingPayout? = null,
        refunded: String? = null,
        longerAtMs: ULong? = null,
    ) = FundingProgress(
        steps = emptyList(),
        failedAtMs = null,
        transactionId = null,
        reference = null,
        deposit = null,
        mismatch = null,
        retrying = false,
        payout = payout,
        refunded = refunded,
        status = status,
        longerAtMs = longerAtMs,
    )

    private fun session(intent: String, direction: FundingDirection, stage: FundingStage, acknowledged: Boolean = false) = FundingSession(
        intent = intent,
        ownerProductId = null,
        direction = direction,
        amount = "50",
        stage = stage,
        openedAtMs = 0uL,
        deadlineMs = 0uL,
        acknowledged = acknowledged,
        providerId = "ramp.dot",
        assignedAtMs = 0uL,
        cancelRequested = false,
        updates = emptyList(),
        started = emptyList(),
        choice = null,
        saved = null,
        refund = null,
    )
}
