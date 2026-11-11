package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import org.junit.Assert.assertTrue
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.FundingBand
import uniffi.truapi.FundingBookEntry
import uniffi.truapi.FundingCancelAvailability
import uniffi.truapi.FundingEndedOutcome
import uniffi.truapi.FundingEnding
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingStatus
import uniffi.truapi.ProductId
import java.math.BigInteger
import kotlin.time.Instant
import uniffi.truapi.FundingDirection as CoreFundingDirection

class TrUAPIHostFundingTest {
    private val ramp = ProductId.parse("ramp.dot").getOrThrow()
    private val sessions = MutableStateFlow<Map<String, FundingBookEntry>>(emptyMap())
    private val history = MutableStateFlow<List<FundingSessionSummary>>(emptyList())
    private val runtime = mockk<FundingRuntime> {
        every { sessions } returns this@TrUAPIHostFundingTest.sessions
        every { history } returns this@TrUAPIHostFundingTest.history
        coEvery { start() } returns Result.success(Unit)
        coEvery { acknowledge(any()) } returns Result.success(true)
    }
    private val funding = TrUAPIHostFunding(
        runtime = runtime,
        sessionScreens = mockk(),
        enabled = true,
    )

    @Test
    fun `opening passes the direction and amount to the core`() = runBlocking {
        coEvery { runtime.openFunding(CoreFundingDirection.OUT, "250") } returns Result.success("intent")

        assertEquals(Result.success("intent"), funding.openFunding(FundingDirection.OUT, Balance(BigInteger("250"))))
    }

    // A session is in flight while its status says so; history leaves it out, and lists the rest newest ended first.
    @Test
    fun `the activity is the sessions in flight, then the history without them`() = runBlocking {
        sessions.value = listOf(
            entry("older", openedAtMs = 1uL, FundingStatus.WaitingForTransfer),
            entry("newer", openedAtMs = 2uL, FundingStatus.ConvertingToCash),
            entry("done", openedAtMs = 0uL, FundingStatus.ToppedUp),
        ).associateBy { it.session.intent }
        history.value = listOf(
            summary("newer", atMs = 9uL, FundingStatus.ToppedUp),
            summary("done", atMs = 3uL, FundingStatus.ToppedUp),
            summary("cancelled", atMs = 5uL, FundingStatus.Failed(FundingFailure.Cancelled), refunded = "10"),
        )

        val read = funding.observeActivity().first()

        coVerify { runtime.start() }
        assertEquals(
            listOf(
                "newer" to FundingActivityStatus.InProgress(FundingActivityStep.Converting),
                "older" to FundingActivityStatus.InProgress(FundingActivityStep.WaitingForTransfer),
            ),
            read.inFlight.map { it.id to it.status },
        )
        assertEquals(
            listOf(
                "cancelled" to FundingActivityStatus.Cancelled(Balance(BigInteger("10"))),
                "done" to FundingActivityStatus.ToppedUp,
            ),
            read.history.map { it.id to it.status },
        )
        assertEquals(Instant.fromEpochMilliseconds(5), read.history.first().date)
        assertEquals(Balance(BigInteger("50000000")), read.history.first().amount)
    }

    @Test
    fun `each session the history shows is acknowledged`() = runBlocking {
        funding.acknowledgeShown(listOf("a", "b"))
        funding.acknowledgeShown(emptyList())

        coVerify(exactly = 2) { runtime.acknowledge(any()) }
        coVerify { runtime.acknowledge("a") }
        coVerify { runtime.acknowledge("b") }
    }

    private fun entry(intent: String, openedAtMs: ULong, status: FundingStatus) = FundingBookEntry(
        session = FundingSession(
            intent = intent,
            ownerProductId = null,
            direction = CoreFundingDirection.IN,
            amount = "50000000",
            stage = FundingStage.Open,
            openedAtMs = openedAtMs,
            deadlineMs = 0uL,
            acknowledged = false,
            providerId = ramp,
            assignedAtMs = 0uL,
            cancel = null,
            updates = emptyList(),
            started = emptyList(),
            choice = null,
            refund = null,
            detail = null,
        ),
        progress = FundingProgress(
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
            status = status,
            longerAtMs = null,
            moneyMoved = false,
            awaitsUserTransfer = false,
            cancel = FundingCancelAvailability.UNAVAILABLE,
        ),
    )

    private fun summary(intent: String, atMs: ULong, status: FundingStatus, refunded: String? = null) = FundingSessionSummary(
        intent = intent,
        ownerProductId = null,
        direction = CoreFundingDirection.IN,
        amount = "50000000",
        stage = FundingStage.Open,
        openedAtMs = 0uL,
        providerId = ramp,
        choice = null,
        status = status,
        payout = null,
        refunded = refunded,
        transactionId = null,
        reference = null,
        moneyMoved = true,
        ended = FundingEnding(atMs, FundingEndedOutcome.Open, null, "50000000", null, null, emptyList(), null),
    )

    @Test
    fun `a build without funding answers no activity and leaves the core alone`() = runBlocking<Unit> {
        val off = TrUAPIHostFunding(runtime = runtime, sessionScreens = mockk(), enabled = false)

        val activity = off.observeActivity().first()

        assertTrue(activity.inFlight.isEmpty() && activity.history.isEmpty())
        coVerify(exactly = 0) { runtime.start() }
    }
}
