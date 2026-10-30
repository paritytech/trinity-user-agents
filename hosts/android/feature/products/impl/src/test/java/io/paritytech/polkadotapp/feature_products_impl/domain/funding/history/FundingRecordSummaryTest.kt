package io.paritytech.polkadotapp.feature_products_impl.domain.funding.history

import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingAssetUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.FundingChoice
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingPayout
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingStatus
import kotlin.time.Instant

/** A record from the core's summary shows the session as a live one does: the summary's status is its outcome. */
class FundingRecordSummaryTest {
    private val released = FundingStage.Released("30", settledAtMs = 2uL)

    @Test
    fun `each ending status maps to its outcome, whatever the stage`() {
        val outcomes = listOf(
            summary(FundingStage.Delivered("40", settledAtMs = 2uL), FundingStatus.ToppedUp),
            summary(released, FundingStatus.Sent),
            summary(released, FundingStatus.PaidOutAfterCancel),
            summary(released, FundingStatus.PayoutFailed(refunded = "25")),
            summary(released, FundingStatus.Cancelled(refunded = "25")),
            summary(FundingStage.Failed(FundingFailure.Refunded, settledAtMs = 2uL), FundingStatus.Refunded),
            summary(FundingStage.Failed(FundingFailure.Declined, settledAtMs = 2uL), FundingStatus.Failed(FundingFailure.Declined)),
        ).map { FundingRecord.of(it)?.outcome }

        assertEquals(
            listOf(
                FundingRecordOutcome.Delivered,
                FundingRecordOutcome.Released,
                FundingRecordOutcome.Released,
                FundingRecordOutcome.Released,
                FundingRecordOutcome.Cancelled,
                FundingRecordOutcome.Refunded,
                FundingRecordOutcome.Failed(FundingFailureReason.Declined),
            ),
            outcomes,
        )
    }

    // A withdrawal whose payout is not reported yet still reads as one sent.
    @Test
    fun `a status still in progress falls back to the stage`() {
        assertEquals(FundingRecordOutcome.Released, FundingRecord.of(summary(released, FundingStatus.PreparingPayout))?.outcome)
    }

    @Test
    fun `a record carries everything the summary keeps`() {
        val record = FundingRecord.of(
            summary(
                released,
                FundingStatus.PayoutFailed(refunded = "25"),
                payout = FundingPayout.Failed("rejected"),
                refunded = "25",
            ),
        )

        assertEquals(
            FundingRecord(
                intent = "fs_1",
                direction = FundingDirection.OUT,
                choice = CHOICE,
                providerId = "ramp.dot",
                requestedAmount = 50.intoBalance(),
                settledAmount = 30.intoBalance(),
                outcome = FundingRecordOutcome.Released,
                payout = FundingRecordPayout.Failed("rejected"),
                refunded = 25.intoBalance(),
                transactionId = "tx",
                reference = "ref",
                openedAt = Instant.fromEpochMilliseconds(1_000),
                settledAt = Instant.fromEpochMilliseconds(2),
            ),
            record,
        )
    }

    // Out, the user receives the quote's receive amount in its asset, at the decimals the quote states.
    @Test
    fun `the quoted amount is the quote's provider-side figure in its asset`() {
        val record = FundingRecord.of(summary(released, FundingStatus.Sent))

        assertEquals(FundingRail.BANK to FundingAssetUnit.of("EUR", 2).format("5100"), record?.rail to record?.quotedAmount)
    }

    @Test
    fun `an open stage makes no record`() {
        assertNull(FundingRecord.of(summary(FundingStage.Open, FundingStatus.WaitingForPayment)))
    }

    private companion object {
        val CHOICE = FundingChoice(
            quote = FundingQuote("q", "50", "5100", "0", "0", null, null, 2u),
            rail = FundingRail.BANK,
            asset = "EUR",
            network = null,
        )
    }

    private fun summary(
        stage: FundingStage,
        status: FundingStatus,
        payout: FundingPayout? = null,
        refunded: String? = null,
    ) = FundingSessionSummary(
        intent = "fs_1",
        ownerProductId = null,
        direction = FundingDirection.OUT,
        amount = "50",
        stage = stage,
        openedAtMs = 1_000uL,
        providerId = "ramp.dot",
        choice = CHOICE,
        status = status,
        payout = payout,
        refunded = refunded,
        transactionId = "tx",
        reference = "ref",
    )
}
