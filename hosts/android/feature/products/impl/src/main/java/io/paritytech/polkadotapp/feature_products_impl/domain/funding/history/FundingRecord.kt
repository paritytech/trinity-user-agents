package io.paritytech.polkadotapp.feature_products_impl.domain.funding.history

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.toReason
import uniffi.truapi.FundingCancel
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingPayout
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.FundingStage
import kotlin.time.Instant

/** One ended funding session as the host keeps it, once the core has handed it over and dropped it. */
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
        /** The record for a session that has ended, or `null` while it is open. */
        fun of(session: FundingSession, progress: FundingProgress?): FundingRecord? {
            // A cancel the user accepted stands; the core marks one the payout overtook, which was sent.
            val (outcome, amount, settledAtMs) = session.stage.ended(cancelled = session.cancel is FundingCancel.Accepted)
                ?: return null

            return FundingRecord(
                intent = session.intent,
                direction = session.direction,
                rail = session.choice?.rail,
                asset = session.choice?.asset,
                providerId = session.providerId,
                requestedAmount = session.amount?.toBalanceOrNull(),
                settledAmount = amount?.toBalanceOrNull(),
                outcome = outcome,
                payout = progress?.payout?.toRecordPayout(),
                refunded = (session.refund?.refunded ?: progress?.refunded)?.toBalanceOrNull(),
                transactionId = progress?.transactionId,
                reference = progress?.reference,
                openedAt = Instant.fromEpochMilliseconds(session.openedAtMs.toLong()),
                settledAt = Instant.fromEpochMilliseconds(settledAtMs.toLong()),
            )
        }

        /**
         * The record the core's summary of a recorded session rebuilds, or `null` for one still open. The summary keeps
         * only how the session ended, so the provider, rail, payout and references are unknown.
         */
        fun of(summary: FundingSessionSummary): FundingRecord? {
            val (outcome, amount, settledAtMs) = summary.stage.ended(cancelled = false) ?: return null
            return FundingRecord(
                intent = summary.intent,
                direction = summary.direction,
                rail = null,
                asset = null,
                providerId = null,
                requestedAmount = summary.amount?.toBalanceOrNull(),
                settledAmount = amount?.toBalanceOrNull(),
                outcome = outcome,
                payout = null,
                refunded = null,
                transactionId = null,
                reference = null,
                openedAt = Instant.fromEpochMilliseconds(summary.openedAtMs.toLong()),
                settledAt = Instant.fromEpochMilliseconds(settledAtMs.toLong()),
            )
        }
    }
}

/** How an ended stage reads in a record: outcome, amount moved and when; `null` while open. */
private fun FundingStage.ended(cancelled: Boolean): Triple<FundingRecordOutcome, String?, ULong>? = when (this) {
    FundingStage.Open -> null
    is FundingStage.Delivered -> Triple(FundingRecordOutcome.Delivered, credited, settledAtMs)
    is FundingStage.Released ->
        Triple(if (cancelled) FundingRecordOutcome.Cancelled else FundingRecordOutcome.Released, debited, settledAtMs)
    is FundingStage.Failed -> Triple(reason.toOutcome(), null, settledAtMs)
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
