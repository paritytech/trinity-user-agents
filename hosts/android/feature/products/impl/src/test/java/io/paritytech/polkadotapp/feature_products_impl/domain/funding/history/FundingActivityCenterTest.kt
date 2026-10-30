package io.paritytech.polkadotapp.feature_products_impl.domain.funding.history

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.FundingSessionEntry
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.FundingBand
import uniffi.truapi.FundingCancel
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingPayout
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingStatus

class FundingActivityCenterTest {
    private val sessions = MutableStateFlow<Map<String, FundingSessionEntry>>(emptyMap())
    private val inProgress = MutableStateFlow<Boolean?>(null)
    private var recorded = emptyList<FundingSessionSummary>()
    private val runtime = mockk<FundingRuntime> {
        every { sessions } returns this@FundingActivityCenterTest.sessions
        every { inProgress } returns this@FundingActivityCenterTest.inProgress
        coEvery { history() } answers { Result.success(recorded) }
        coEvery { start() } returns Result.success(Unit)
        coEvery { acknowledge(any()) } returns Result.success(true)
    }
    private val center = FundingActivityCenter(runtime)

    @Test
    fun `an ended top-up is listed and acknowledged`() = runBlocking {
        serve(session("in", FundingDirection.IN, FundingStage.Delivered("50", settledAtMs = 1uL)) to progress(FundingStatus.ToppedUp))

        val activity = center.observe().first()

        coVerify(exactly = 1) { runtime.acknowledge("in") }
        assertEquals(listOf(FundingActivityStatus.ToppedUp), activity.history.map { it.status })
    }

    // The core keeps a released withdrawal until its payout is reported, so the host need not hold it back.
    @Test
    fun `a released withdrawal is acknowledged, and stays in progress until its payout`() = runBlocking {
        serve(session("out", FundingDirection.OUT, FundingStage.Released("50", settledAtMs = 1uL)) to progress(FundingStatus.PreparingPayout))
        recorded = listOf(summary("out", FundingStage.Released("50", settledAtMs = 1uL)))

        val activity = center.observe().first()

        coVerify { runtime.acknowledge("out") }
        assertEquals(listOf(FundingActivityStep.PreparingPayout), activity.inFlight.map { (it.status as FundingActivityStatus.InProgress).step })
        assertEquals(emptyList<String>(), activity.history.map { it.id })
    }

    @Test
    fun `an acknowledged session is not acknowledged again`() = runBlocking {
        serve(session("in", FundingDirection.IN, FundingStage.Delivered("50", settledAtMs = 1uL), acknowledged = true) to progress(FundingStatus.ToppedUp))

        center.observe().first()

        coVerify(exactly = 0) { runtime.acknowledge(any()) }
    }

    @Test
    fun `a session is acknowledged once however often the list is built`() = runBlocking {
        serve(session("in", FundingDirection.IN, FundingStage.Delivered("50", settledAtMs = 1uL)) to progress(FundingStatus.ToppedUp))

        center.observe().first()
        center.observe().first()

        coVerify(exactly = 1) { runtime.acknowledge("in") }
    }

    // A late payout or refund report hands the session back unacknowledged, and its row has to follow.
    @Test
    fun `a failed payout's refund shows what came back, and is acknowledged again`() = runBlocking {
        val released = FundingStage.Released("50", settledAtMs = 1uL)
        serve(session("out", FundingDirection.OUT, released) to progress(FundingStatus.PayoutFailed(refunded = null), FundingPayout.Failed("rejected")))
        center.observe().first()

        serve(
            session("out", FundingDirection.OUT, released) to
                progress(FundingStatus.PayoutFailed(refunded = "45"), FundingPayout.Failed("rejected"), refunded = "45"),
        )
        val activity = center.observe().first()

        coVerify(exactly = 2) { runtime.acknowledge("out") }
        assertEquals(listOf(FundingActivityStatus.PayoutFailed(45.intoBalance())), activity.history.map { it.status })
    }

    // The core keeps every ended session; one not pushed in this process reads from its summary.
    @Test
    fun `ended sessions the core did not push read from its history, and a pushed session wins`() = runBlocking {
        serve(session("pushed", FundingDirection.IN, FundingStage.Delivered("50", settledAtMs = 3uL)) to progress(FundingStatus.ToppedUp))
        recorded = listOf(
            summary("pushed", FundingStage.Delivered("40", settledAtMs = 3uL)),
            summary("older", FundingStage.Delivered("30", settledAtMs = 2uL)),
            summary("failed", FundingStage.Failed(FundingFailure.Declined, settledAtMs = 1uL)),
        )

        val activity = center.observe().first()

        assertEquals(listOf("pushed", "older", "failed"), activity.history.map { it.id })
        assertEquals(listOf(50.intoBalance(), 30.intoBalance(), 50.intoBalance()), activity.history.map { it.amount })
        coVerify(exactly = 1) { runtime.acknowledge(any()) }
    }

    // The core loads its sessions once funding resumes, which it says by pushing whether funding is in progress.
    @Test
    fun `the history is read again once funding resumes`() = runBlocking {
        serve()

        val ids = withTimeout(2_000) {
            center.observe().take(2).map { activity ->
                if (activity.history.isEmpty()) {
                    recorded = listOf(summary("loaded", FundingStage.Delivered("30", settledAtMs = 2uL)))
                    inProgress.value = false
                }
                activity.history.map { it.id }
            }.toList()
        }

        assertEquals(listOf(emptyList(), listOf("loaded")), ids)
    }

    @Test
    fun `an ended session reads in full while the core pushed it, else from its summary`() = runBlocking {
        serve(session("pushed", FundingDirection.IN, FundingStage.Delivered("50", settledAtMs = 3uL)) to progress(FundingStatus.ToppedUp))
        recorded = listOf(summary("older", FundingStage.Delivered("30", settledAtMs = 2uL)))

        assertEquals("ramp.dot", center.ended("pushed")?.providerId)
        assertEquals(30.intoBalance(), center.ended("older")?.settledAmount)
        assertEquals(null, center.ended("missing"))
        // A pushed session needs no read.
        coVerify(exactly = 2) { runtime.history() }
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

    @Test
    fun `a cancel the provider decides shows in flight at each step`() = runBlocking {
        serve(
            session("requested", FundingDirection.OUT, FundingStage.Open) to progress(FundingStatus.CancelRequested),
            session("offered", FundingDirection.OUT, FundingStage.Open) to progress(FundingStatus.CancelOffered(fee = "5", refund = "45")),
            session("refused", FundingDirection.IN, FundingStage.Open) to progress(FundingStatus.CancelRefused("already converted")),
            session("cancelling", FundingDirection.OUT, FundingStage.Open) to progress(FundingStatus.Cancelling),
        )

        val activity = center.observe().first()

        assertEquals(
            mapOf(
                "requested" to FundingActivityStep.CancelRequested,
                "offered" to FundingActivityStep.CancelOffered,
                "refused" to FundingActivityStep.CancelRefused("already converted"),
                "cancelling" to FundingActivityStep.Cancelling,
            ),
            activity.inFlight.associate { it.id to (it.status as FundingActivityStatus.InProgress).step },
        )
    }

    // An accepted cancel ends a withdrawal released, so only the session's cancel tells it from a sent one.
    @Test
    fun `a withdrawal cancelled on the provider's terms is kept as cancelled with its refund`() = runBlocking {
        val cancelled = session(
            "out",
            FundingDirection.OUT,
            FundingStage.Released("50", settledAtMs = 1uL),
            cancel = FundingCancel.Accepted(fee = "5", refund = "45"),
        )
        serve(cancelled to progress(FundingStatus.Cancelled(refunded = "45"), refunded = "45"))

        val activity = center.observe().first()

        assertEquals(listOf(FundingActivityStatus.Cancelled(45.intoBalance())), activity.history.map { it.status })
    }

    /** The payout went through before the provider could act on the accepted cancel, so the withdrawal stands. */
    @Test
    fun `a withdrawal paid out after the user accepted a cancel is kept as sent`() = runBlocking {
        val paidOut = session(
            "out",
            FundingDirection.OUT,
            FundingStage.Released("50", settledAtMs = 1uL),
            cancel = FundingCancel.Overtaken(fee = "5", refund = "45"),
        )
        serve(paidOut to progress(FundingStatus.PaidOutAfterCancel, payout = FundingPayout.PaidOut))

        val activity = center.observe().first()

        assertEquals(listOf(FundingActivityStatus.Sent), activity.history.map { it.status })
    }

    @Test
    fun `the list is built again when the core pushes a session`() = runBlocking {
        serve(session("in", FundingDirection.IN, FundingStage.Open) to progress(FundingStatus.WaitingForPayment))

        val steps = withTimeout(2_000) {
            center.observe().take(2).map { activity ->
                val step = (activity.inFlight.single().status as FundingActivityStatus.InProgress).step
                if (step == FundingActivityStep.WaitingForPayment) {
                    serve(session("in", FundingDirection.IN, FundingStage.Open) to progress(FundingStatus.TakingLonger))
                }
                step
            }.toList()
        }

        assertEquals(listOf(FundingActivityStep.WaitingForPayment, FundingActivityStep.TakingLonger), steps)
        coVerify(exactly = 1) { runtime.start() }
    }

    private fun summary(intent: String, stage: FundingStage) = FundingSessionSummary(
        intent = intent,
        ownerProductId = null,
        direction = FundingDirection.IN,
        amount = "50",
        stage = stage,
        openedAtMs = 0uL,
    )

    private fun serve(vararg entries: Pair<FundingSession, FundingProgress>) {
        sessions.value = entries.associate { (session, progress) -> session.intent to FundingSessionEntry(session, progress) }
    }

    private fun progress(
        status: FundingStatus,
        payout: FundingPayout? = null,
        refunded: String? = null,
    ) = FundingProgress(
        steps = emptyList(),
        band = FundingBand.AWAITING_PAYMENT,
        lastHeardAtMs = null,
        failedAtMs = null,
        transactionId = null,
        reference = null,
        deposit = null,
        mismatch = null,
        retrying = false,
        payout = payout,
        refunded = refunded,
        status = status,
        longerAtMs = null,
        moneyMoved = false,
    )

    private fun session(
        intent: String,
        direction: FundingDirection,
        stage: FundingStage,
        acknowledged: Boolean = false,
        cancel: FundingCancel? = null,
    ) = FundingSession(
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
        cancel = cancel,
        updates = emptyList(),
        started = emptyList(),
        choice = null,
        refund = null,
    )
}
