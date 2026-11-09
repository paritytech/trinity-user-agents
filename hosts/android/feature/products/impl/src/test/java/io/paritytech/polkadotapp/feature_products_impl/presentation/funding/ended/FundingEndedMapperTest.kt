package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.ended

import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingAssetUnit
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCash
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingDotState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingTimeline
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.FundingChoice
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingEndedOutcome
import uniffi.truapi.FundingEndedView
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingPayout
import uniffi.truapi.FundingPayoutState
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingStatus
import uniffi.truapi.FundingStep
import uniffi.truapi.ProductId
import kotlin.time.Instant
import io.paritytech.polkadotapp.common.R as RCommon

// The core says how a session ended, what it moved and where it stopped; these check the screen reads those facts.
class FundingEndedMapperTest {
    private val ramp = ProductId.parse("ramp.dot").getOrThrow()
    private val mapper = FundingEndedMapper(FundingCash(symbol = "CASH", precision = 6))

    @Test
    fun `a top-up that went through is signed, green, with every milestone done`() {
        val ended = map(summary(FundingDirection.IN, delivered(), FundingEndedOutcome.Succeeded))

        assertEquals(true, ended.succeeded)
        assertEquals(FundingEndedIcon.IN, ended.icon)
        assertEquals("+$50", ended.figure)
        assertEquals(FundingEndedTone.SUCCESS, ended.figureTone)
        assertEquals(List(FundingTimeline.DOTS) { FundingDotState.DONE }, ended.timeline.dots)
        assertNull(ended.note)
        assertEquals(FundingEndedAction.CLOSE, ended.action)
    }

    @Test
    fun `a declined card payment is red on the step the core says it stopped at, and says why`() {
        val ended = map(
            summary(FundingDirection.IN, failed(FundingFailure.Declined), FundingEndedOutcome.Failed(FundingFailure.Declined), stoppedAt = FundingStep.RECEIVED),
        )

        assertEquals(FundingEndedIcon.FAILED, ended.icon)
        assertEquals("$50", ended.figure)
        assertEquals(FundingEndedTone.TERTIARY, ended.figureTone)
        assertEquals(FundingDotState.FAILED, ended.timeline.dots[FundingTimeline.MILESTONE_GAP])
        assertEquals(1, ended.timeline.failedLabel)
        assertEquals(RCommon.string.funding_ended_step_payment_declined, ended.timeline.labels[1])
        assertEquals(FundingEndedText.Res(RCommon.string.funding_ended_card_declined, emptyList()), ended.note)
        assertEquals(FundingEndedAction.START_OVER, ended.action)
    }

    @Test
    fun `a refund the provider made after receiving the payment is red on the last step`() {
        val ended = map(
            summary(FundingDirection.IN, failed(FundingFailure.Refunded), FundingEndedOutcome.Failed(FundingFailure.Refunded), stoppedAt = FundingStep.ADDED),
        )

        assertEquals(FundingDotState.FAILED, ended.timeline.dots.last())
        assertEquals(RCommon.string.funding_activity_refunded, ended.timeline.labels.last())
        assertEquals(FundingEndedText.Res(RCommon.string.funding_ended_refunded_card, listOf(PAID)), ended.note)
        assertEquals(FundingEndedRow.Value(RCommon.string.funding_ended_paid, FundingEndedText.Plain(PAID)), ended.rows.first())
    }

    @Test
    fun `a failed payout stops on the payout and says what came back`() {
        val ended = map(
            summary(
                FundingDirection.OUT,
                released(),
                FundingEndedOutcome.PayoutFailed("rejected"),
                payout = FundingPayout.Failed("rejected"),
                payoutState = FundingPayoutState.FAILED,
                refunded = "49000000",
                stoppedAt = FundingStep.SENT,
            ),
        )

        assertEquals(RCommon.string.funding_activity_payout_failed, ended.timeline.labels.last())
        assertEquals(FundingDotState.FAILED, ended.timeline.dots.last())
        assertEquals(FundingEndedText.Res(RCommon.string.funding_ended_withdraw_refunded, listOf("$49 CASH")), ended.note)
    }

    @Test
    fun `a released withdrawal waits on its payout`() {
        val ended = map(summary(FundingDirection.OUT, released(), FundingEndedOutcome.Succeeded, payoutState = FundingPayoutState.PENDING))

        assertEquals(true, ended.succeeded)
        assertEquals("-$50", ended.figure)
        assertEquals(FundingDotState.CURRENT, ended.timeline.dots[5])
        assertTrue(
            ended.rows.contains(
                FundingEndedRow.Value(RCommon.string.funding_ended_payout, FundingEndedText.Res(RCommon.string.funding_ended_payout_pending, emptyList())),
            ),
        )
    }

    @Test
    fun `rows hold only what the summary knows`() {
        val ended = map(
            summary(FundingDirection.IN, delivered(), FundingEndedOutcome.Succeeded, differingRequested = "60000000", transactionId = "tx-1"),
        )

        assertEquals(
            listOf(
                FundingEndedRow.Value(RCommon.string.funding_ended_paid, FundingEndedText.Plain(PAID)),
                FundingEndedRow.Settled(RCommon.string.funding_ended_credited, "$50 CASH", requested = "$60 CASH"),
                FundingEndedRow.Value(RCommon.string.funding_ended_method, FundingEndedText.Res(RCommon.string.funding_rail_card, emptyList())),
                FundingEndedRow.Provider(ramp),
                FundingEndedRow.Copy(RCommon.string.funding_ended_transaction_id, "tx-1"),
                FundingEndedRow.Moment(RCommon.string.funding_ended_started, OPENED),
                FundingEndedRow.Moment(RCommon.string.funding_ended_completed, SETTLED),
            ),
            ended.rows,
        )
    }

    @Test
    fun `a session's quote shows what was paid and the core's name for its crypto network`() {
        val choice = FundingChoice(
            quote = FundingQuote("q", "2500000", "50000000", "0", "0", null, null, 6u),
            rail = FundingRail.CRYPTO,
            asset = "USDC",
            network = "polkadot",
        )
        val ended = map(summary(FundingDirection.IN, delivered(), FundingEndedOutcome.Succeeded, choice = choice))

        assertEquals(
            listOf(
                FundingEndedRow.Value(RCommon.string.funding_ended_sent, FundingEndedText.Plain("2.5 USDC")),
                FundingEndedRow.Value(
                    RCommon.string.funding_ended_method,
                    FundingEndedText.Res(RCommon.string.funding_ended_method_crypto_network, listOf("Polkadot")),
                ),
            ),
            ended.rows.filterIsInstance<FundingEndedRow.Value>(),
        )
    }

    @Test
    fun `a session that has not ended has no detail`() {
        assertNull(mapper.map(summary(FundingDirection.IN, FundingStage.Open, FundingEndedOutcome.Open)))
    }

    private fun map(summary: FundingSessionSummary) = requireNotNull(mapper.map(summary))

    private fun delivered() = FundingStage.Delivered(credited = "50000000", settledAtMs = SETTLED_MS)

    private fun released() = FundingStage.Released(debited = "50000000", settledAtMs = SETTLED_MS)

    private fun failed(reason: FundingFailure) = FundingStage.Failed(reason = reason, settledAtMs = SETTLED_MS)

    private fun summary(
        direction: FundingDirection,
        stage: FundingStage,
        outcome: FundingEndedOutcome,
        payout: FundingPayout? = null,
        payoutState: FundingPayoutState? = null,
        refunded: String? = null,
        differingRequested: String? = null,
        stoppedAt: FundingStep? = null,
        transactionId: String? = null,
        choice: FundingChoice? = CARD,
    ) = FundingSessionSummary(
        intent = "fs_1",
        ownerProductId = null,
        direction = direction,
        amount = "50000000",
        stage = stage,
        openedAtMs = OPENED_MS,
        providerId = ramp,
        choice = choice,
        status = FundingStatus.ToppedUp,
        payout = payout,
        refunded = refunded,
        transactionId = transactionId,
        reference = null,
        ended = FundingEndedView(
            outcome = outcome,
            payoutState = payoutState,
            cashAmount = "50000000",
            differingRequested = differingRequested,
            stoppedAt = stoppedAt,
        ),
    )

    private companion object {
        const val OPENED_MS = 1_000uL
        const val SETTLED_MS = 2_000uL
        val CARD = FundingChoice(
            quote = FundingQuote("q", "5100", "50000000", "0", "0", null, null, 2u),
            rail = FundingRail.CARD,
            asset = "USD",
            network = null,
        )
        val PAID = FundingAssetUnit.of("USD", 2).format("5100")
        val OPENED = Instant.fromEpochMilliseconds(OPENED_MS.toLong())
        val SETTLED = Instant.fromEpochMilliseconds(SETTLED_MS.toLong())
    }
}
