package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.ended

import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCash
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.FundingRecord
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.FundingRecordOutcome
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.FundingRecordPayout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingRail
import kotlin.time.Instant
import io.paritytech.polkadotapp.common.R as RCommon

class FundingEndedMapperTest {
    private val mapper = FundingEndedMapper(FundingCash(symbol = "CASH", precision = 6))

    @Test
    fun `a top-up that went through is signed, green, and has no steps`() {
        val ended = mapper.map(record(FundingDirection.IN, FundingRecordOutcome.Delivered))

        assertEquals(true, ended.succeeded)
        assertEquals(FundingEndedIcon.IN, ended.icon)
        assertEquals("+$50", ended.figure)
        assertEquals(FundingEndedTone.SUCCESS, ended.figureTone)
        assertTrue(ended.steps.isEmpty())
        assertNull(ended.note)
        assertEquals(FundingEndedAction.CLOSE, ended.action)
    }

    @Test
    fun `a declined card payment stops on the payment step and says why`() {
        val ended = mapper.map(record(FundingDirection.IN, FundingRecordOutcome.Failed(FundingFailureReason.Declined)))

        assertEquals(FundingEndedIcon.FAILED, ended.icon)
        assertEquals("$50", ended.figure)
        assertEquals(FundingEndedTone.TERTIARY, ended.figureTone)
        assertEquals(
            listOf(FundingEndedStepState.DONE, FundingEndedStepState.FAILED, FundingEndedStepState.UPCOMING, FundingEndedStepState.UPCOMING, FundingEndedStepState.UPCOMING),
            ended.steps.map { it.state },
        )
        assertEquals(RCommon.string.funding_ended_step_payment_declined, ended.steps[1].title)
        assertEquals(FundingEndedText.Res(RCommon.string.funding_ended_card_declined, emptyList()), ended.note)
        assertEquals(FundingEndedAction.START_OVER, ended.action)
    }

    @Test
    fun `a failed payout stops on the payout and says what came back`() {
        val ended = mapper.map(
            record(FundingDirection.OUT, FundingRecordOutcome.Released, payout = FundingRecordPayout.Failed("rejected"), refunded = 49_000_000),
        )

        assertEquals(RCommon.string.funding_activity_payout_failed, ended.steps.last().title)
        assertEquals(FundingEndedStepState.FAILED, ended.steps.last().state)
        assertEquals(FundingEndedText.Res(RCommon.string.funding_ended_withdraw_refunded, listOf("$49 CASH")), ended.note)
    }

    @Test
    fun `a released withdrawal waits on its payout`() {
        val ended = mapper.map(record(FundingDirection.OUT, FundingRecordOutcome.Released))

        assertEquals(true, ended.succeeded)
        assertEquals("-$50", ended.figure)
        assertEquals(FundingEndedStepState.CURRENT, ended.steps.last().state)
        assertTrue(
            ended.rows.contains(
                FundingEndedRow.Value(RCommon.string.funding_ended_payout, FundingEndedText.Res(RCommon.string.funding_ended_payout_pending, emptyList())),
            ),
        )
    }

    @Test
    fun `rows hold only what the record knows`() {
        val ended = mapper.map(
            record(FundingDirection.IN, FundingRecordOutcome.Delivered, requested = 60_000_000, transactionId = "tx-1"),
        )

        assertEquals(
            listOf(
                FundingEndedRow.Settled(RCommon.string.funding_ended_credited, "$50 CASH", requested = "$60 CASH"),
                FundingEndedRow.Value(RCommon.string.funding_ended_method, FundingEndedText.Res(RCommon.string.funding_rail_card, emptyList())),
                FundingEndedRow.Provider("ramp.dot"),
                FundingEndedRow.Copy(RCommon.string.funding_ended_transaction_id, "tx-1"),
                FundingEndedRow.Moment(RCommon.string.funding_ended_started, OPENED),
                FundingEndedRow.Moment(RCommon.string.funding_ended_completed, SETTLED),
            ),
            ended.rows,
        )
    }

    private fun record(
        direction: FundingDirection,
        outcome: FundingRecordOutcome,
        payout: FundingRecordPayout? = null,
        refunded: Long? = null,
        requested: Long = 50_000_000,
        transactionId: String? = null,
    ) = FundingRecord(
        intent = "fs_1",
        direction = direction,
        rail = FundingRail.CARD,
        asset = "USD",
        providerId = "ramp.dot",
        requestedAmount = requested.intoBalance(),
        settledAmount = 50_000_000L.intoBalance().takeIf { outcome !is FundingRecordOutcome.Failed },
        outcome = outcome,
        payout = payout,
        refunded = refunded?.intoBalance(),
        transactionId = transactionId,
        reference = null,
        openedAt = OPENED,
        settledAt = SETTLED,
    )

    private companion object {
        val OPENED = Instant.fromEpochMilliseconds(1_000)
        val SETTLED = Instant.fromEpochMilliseconds(2_000)
    }
}
