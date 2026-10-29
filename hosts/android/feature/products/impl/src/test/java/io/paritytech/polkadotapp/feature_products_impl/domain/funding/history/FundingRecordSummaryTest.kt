package io.paritytech.polkadotapp.feature_products_impl.domain.funding.history

import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.FundingStage
import kotlin.time.Instant

/** A record rebuilt from the core's summary keeps how the session ended and nothing the summary does not carry. */
class FundingRecordSummaryTest {
    @Test
    fun `each ended stage maps to its outcome and amount`() {
        val records = listOf(
            FundingStage.Delivered("40", settledAtMs = 2uL),
            FundingStage.Released("30", settledAtMs = 2uL),
            FundingStage.Failed(FundingFailure.Refunded, settledAtMs = 2uL),
            FundingStage.Failed(FundingFailure.Declined, settledAtMs = 2uL),
        ).map { FundingRecord.of(summary(it)) }

        assertEquals(
            listOf(
                FundingRecordOutcome.Delivered to 40.intoBalance(),
                FundingRecordOutcome.Released to 30.intoBalance(),
                FundingRecordOutcome.Refunded to null,
                FundingRecordOutcome.Failed(FundingFailureReason.Declined) to null,
            ),
            records.map { it?.outcome to it?.settledAmount },
        )
    }

    @Test
    fun `a rebuilt record has the summary's times and amount and nothing else`() {
        val record = FundingRecord.of(summary(FundingStage.Delivered("40", settledAtMs = 2_000uL)))

        assertEquals(
            FundingRecord(
                intent = "fs_1",
                direction = FundingDirection.OUT,
                rail = null,
                asset = null,
                providerId = null,
                requestedAmount = 50.intoBalance(),
                settledAmount = 40.intoBalance(),
                outcome = FundingRecordOutcome.Delivered,
                payout = null,
                refunded = null,
                transactionId = null,
                reference = null,
                openedAt = Instant.fromEpochMilliseconds(1_000),
                settledAt = Instant.fromEpochMilliseconds(2_000),
            ),
            record,
        )
    }

    @Test
    fun `an open stage rebuilds nothing`() {
        assertNull(FundingRecord.of(summary(FundingStage.Open)))
    }

    private fun summary(stage: FundingStage) = FundingSessionSummary(
        intent = "fs_1",
        ownerProductId = null,
        direction = FundingDirection.OUT,
        amount = "50",
        stage = stage,
        openedAtMs = 1_000uL,
    )
}
