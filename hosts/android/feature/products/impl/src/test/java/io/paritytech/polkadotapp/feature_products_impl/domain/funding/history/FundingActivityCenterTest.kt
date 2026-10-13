package io.paritytech.polkadotapp.feature_products_impl.domain.funding.history

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_impl.data.repository.FundingHistoryRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.FundingChoice
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingPayout
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingProgressStep
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingStep
import kotlin.time.Clock
import kotlin.time.Duration.Companion.hours
import kotlin.time.Instant

class FundingActivityCenterTest {
    private val runtime = mockk<FundingRuntime> {
        every { sessionChanges() } returns emptyFlow()
        coEvery { acknowledge(any()) } returns Result.success(true)
    }
    private val saved = mutableListOf<FundingRecord>()
    private val history = mockk<FundingHistoryRepository> {
        val record = slot<FundingRecord>()
        coEvery { save(capture(record)) } answers { saved += record.captured; Result.success(Unit) }
        coEvery { records() } answers { Result.success(saved.distinctBy { it.intent }) }
    }
    private val center = FundingActivityCenter(runtime, history)

    @Test
    fun `an ended top-up is written once and then acknowledged`() = runBlocking {
        coEvery { runtime.sessions() } returns listOf(session("in", FundingDirection.IN, FundingStage.Delivered("50", settledAtMs = recently())))
        coEvery { runtime.progress("in") } returns null

        val activity = center.observe().first()

        assertEquals(listOf("in"), saved.map { it.intent })
        coVerify { runtime.acknowledge("in") }
        assertEquals(listOf(FundingActivityStatus.ToppedUp), activity.history.map { it.status })
    }

    @Test
    fun `an outbound session waits for its payout before the core may drop it`() = runBlocking {
        coEvery { runtime.sessions() } returns listOf(session("out", FundingDirection.OUT, FundingStage.Released("50", settledAtMs = recently())))
        coEvery { runtime.progress("out") } returns null

        center.observe().first()

        assertEquals(listOf("out"), saved.map { it.intent })
        coVerify(exactly = 0) { runtime.acknowledge(any()) }
    }

    @Test
    fun `an outbound session is acknowledged once its payout failed, and reads as payout failed`() = runBlocking {
        coEvery { runtime.sessions() } returns listOf(session("out", FundingDirection.OUT, FundingStage.Released("50", settledAtMs = recently())))
        coEvery { runtime.progress("out") } returns progress(FundingPayout.Failed("bank rejected"))

        val activity = center.observe().first()

        coVerify { runtime.acknowledge("out") }
        assertEquals(listOf(FundingActivityStatus.PayoutFailed), activity.history.map { it.status })
    }

    @Test
    fun `an outbound session waiting for its payout stays in progress, preparing it`() = runBlocking {
        val released = session("out", FundingDirection.OUT, FundingStage.Released("50", settledAtMs = recently()), choice(FundingRail.BANK))
        coEvery { runtime.sessions() } returns listOf(released)
        coEvery { runtime.progress("out") } returns null

        val activity = center.observe().first()

        assertEquals(listOf(FundingActivityStep.PreparingPayout), activity.inFlight.map { (it.status as FundingActivityStatus.InProgress).step })
        assertEquals(emptyList<String>(), activity.history.map { it.id })
    }

    @Test
    fun `value in waits on the rail's own payment, then converts once the provider has it`() = runBlocking {
        val crypto = session("crypto", FundingDirection.IN, FundingStage.Open, choice(FundingRail.CRYPTO))
        val card = session("card", FundingDirection.IN, FundingStage.Open, choice(FundingRail.CARD))
        coEvery { runtime.sessions() } returns listOf(crypto, card)
        coEvery { runtime.progress("crypto") } returns null
        coEvery { runtime.progress("card") } returns progress(null, paidAtMs = 1uL)

        val steps = center.observe().first().inFlight.associate { it.id to (it.status as FundingActivityStatus.InProgress).step }

        assertEquals(mapOf("crypto" to FundingActivityStep.WaitingForDeposit, "card" to FundingActivityStep.Converting), steps)
    }

    @Test
    fun `a day without a payout is enough to let the core drop it`() {
        val record = requireNotNull(FundingRecord.of(session("out", FundingDirection.OUT, FundingStage.Released("50", settledAtMs = 0uL)), null))

        assertEquals(
            listOf(false, true),
            listOf(record.isFinal(Instant.fromEpochMilliseconds(0) + 23.hours), record.isFinal(Instant.fromEpochMilliseconds(0) + 24.hours)),
        )
        assertEquals(50.intoBalance(), record.settledAmount)
    }

    private fun recently(): ULong = Clock.System.now().toEpochMilliseconds().toULong()

    private fun progress(payout: FundingPayout?, paidAtMs: ULong? = null) = FundingProgress(
        steps = listOf(FundingProgressStep(FundingStep.PAYMENT, paidAtMs)),
        failedAtMs = null,
        transactionId = null,
        reference = null,
        deposit = null,
        mismatch = null,
        retrying = false,
        payout = payout,
    )

    private fun choice(rail: FundingRail) = FundingChoice(
        quote = FundingQuote(
            quoteId = "q",
            sendAmount = "50",
            receiveAmount = "50",
            providerFee = "0",
            networkFee = "0",
            etaSecs = null,
            expiresAt = null,
        ),
        rail = rail,
        asset = "USDT",
        amount = "50",
    )

    private fun session(intent: String, direction: FundingDirection, stage: FundingStage, choice: FundingChoice? = null) = FundingSession(
        intent = intent,
        ownerProductId = null,
        direction = direction,
        amount = "50",
        stage = stage,
        openedAtMs = 0uL,
        deadlineMs = 0uL,
        acknowledged = false,
        providerId = "ramp.dot",
        cancelRequested = false,
        updates = emptyList(),
        choice = choice,
        saved = null,
    )
}
