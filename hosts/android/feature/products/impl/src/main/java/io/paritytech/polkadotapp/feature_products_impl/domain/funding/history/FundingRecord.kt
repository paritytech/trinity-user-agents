package io.paritytech.polkadotapp.feature_products_impl.domain.funding.history

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.toReason
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingPayout
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingStatus
import kotlin.time.Instant

/** One ended funding session as the history shows it, from the core's summary of it. */
data class FundingRecord(
    val intent: String,
    val direction: FundingDirection,
    val rail: FundingRail?,
    val asset: String?,
    val providerId: String?,
    val requestedAmount: Balance?,
    val settledAmount: Balance?,
    val outcome: FundingRecordOutcome,
    val payout: FundingRecordPayout?,
    /** What the provider's refund of a failed payout or an accepted cancel returned to the balance, once its claim is final. */
    val refunded: Balance?,
    val transactionId: String?,
    val reference: String?,
    val openedAt: Instant,
    val settledAt: Instant,
) {
    companion object {
        /**
         * The record of a session the core's history summarises, or `null` for one still open. Its outcome is the
         * summary's status, the one a live session shows; the stage gives only the amount moved and when.
         */
        fun of(summary: FundingSessionSummary): FundingRecord? {
            val (stageOutcome, amount, settledAtMs) = summary.stage.ended() ?: return null
            return FundingRecord(
                intent = summary.intent,
                direction = summary.direction,
                rail = summary.rail,
                asset = summary.asset,
                providerId = summary.providerId,
                requestedAmount = summary.amount?.toBalanceOrNull(),
                settledAmount = amount?.toBalanceOrNull(),
                outcome = summary.status.toOutcome() ?: stageOutcome,
                payout = summary.payout?.toRecordPayout(),
                refunded = summary.refunded?.toBalanceOrNull(),
                transactionId = summary.transactionId,
                reference = summary.reference,
                openedAt = Instant.fromEpochMilliseconds(summary.openedAtMs.toLong()),
                settledAt = Instant.fromEpochMilliseconds(settledAtMs.toLong()),
            )
        }
    }
}

/**
 * How an ended stage reads, for a status still in progress (a withdrawal whose payout is not reported yet): outcome,
 * amount moved and when; `null` while open.
 */
private fun FundingStage.ended(): Triple<FundingRecordOutcome, String?, ULong>? = when (this) {
    FundingStage.Open -> null
    is FundingStage.Delivered -> Triple(FundingRecordOutcome.Delivered, credited, settledAtMs)
    is FundingStage.Released -> Triple(FundingRecordOutcome.Released, debited, settledAtMs)
    is FundingStage.Failed -> Triple(reason.toOutcome(), null, settledAtMs)
}

/** The outcome an ending status names; `null` for one still in progress, such as a payout not yet reported. */
private fun FundingStatus.toOutcome(): FundingRecordOutcome? = when (this) {
    FundingStatus.ToppedUp -> FundingRecordOutcome.Delivered
    FundingStatus.Sent, FundingStatus.PaidOutAfterCancel, is FundingStatus.PayoutFailed -> FundingRecordOutcome.Released
    FundingStatus.Refunded -> FundingRecordOutcome.Refunded
    is FundingStatus.Cancelled -> FundingRecordOutcome.Cancelled
    is FundingStatus.Failed -> reason.toOutcome()
    else -> null
}

sealed interface FundingRecordOutcome {
    data object Delivered : FundingRecordOutcome

    data object Released : FundingRecordOutcome

    data object Refunded : FundingRecordOutcome

    /** Value out, cancelled on the provider's terms after the balance was debited; the refund is the record's `refunded`. */
    data object Cancelled : FundingRecordOutcome

    /** Kept with an `Other` failure's message, so a stored one reads back the same. */
    data class Failed(val reason: FundingFailureReason) : FundingRecordOutcome
}

sealed interface FundingRecordPayout {
    data object PaidOut : FundingRecordPayout

    data class Failed(val reason: String) : FundingRecordPayout
}

private fun String.toBalanceOrNull(): Balance? = toBigIntegerOrNull()?.intoBalance()

private fun FundingPayout.toRecordPayout(): FundingRecordPayout = when (this) {
    FundingPayout.PaidOut -> FundingRecordPayout.PaidOut
    is FundingPayout.Failed -> FundingRecordPayout.Failed(reason)
}

private fun FundingFailure.toOutcome(): FundingRecordOutcome = when (this) {
    FundingFailure.Refunded -> FundingRecordOutcome.Refunded
    else -> FundingRecordOutcome.Failed(toReason())
}
