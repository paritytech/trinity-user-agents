package io.paritytech.polkadotapp.feature_products_impl.domain.funding.history

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingPayout
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingStage
import kotlin.time.Duration.Companion.hours
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
    val transactionId: String?,
    val reference: String?,
    val openedAt: Instant,
    val settledAt: Instant,
) {
    companion object {
        /** Outbound value waits this long for the provider's payout outcome before the core may drop it. */
        val PAYOUT_WAIT = 24.hours

        /** The record for a session that has ended, or `null` while it is open. */
        fun of(session: FundingSession, progress: FundingProgress?): FundingRecord? {
            val (outcome, amount, settledAtMs) = when (val stage = session.stage) {
                FundingStage.Open -> return null
                is FundingStage.Delivered -> Triple(FundingRecordOutcome.Delivered, stage.credited, stage.settledAtMs)
                is FundingStage.Released -> Triple(FundingRecordOutcome.Released, stage.debited, stage.settledAtMs)
                is FundingStage.Failed -> Triple(stage.reason.toOutcome(), null, stage.settledAtMs)
            }

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
                transactionId = progress?.transactionId,
                reference = progress?.reference,
                openedAt = Instant.fromEpochMilliseconds(session.openedAtMs.toLong()),
                settledAt = Instant.fromEpochMilliseconds(settledAtMs.toLong()),
            )
        }
    }

    /** Outbound value is done once the provider says it paid out; until then, or for a day, the core keeps it. */
    fun isFinal(now: Instant): Boolean {
        val awaitsPayout = direction == FundingDirection.OUT && outcome == FundingRecordOutcome.Released && payout == null
        return !awaitsPayout || now - settledAt >= PAYOUT_WAIT
    }
}

sealed interface FundingRecordOutcome {
    data object Delivered : FundingRecordOutcome

    data object Released : FundingRecordOutcome

    data object Refunded : FundingRecordOutcome

    /** [code] is a stable name for the failure, so a stored one reads back the same. */
    data class Failed(val code: String) : FundingRecordOutcome
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
    FundingFailure.RegionUnavailable -> FundingRecordOutcome.Failed("regionUnavailable")
    FundingFailure.VerificationRequired -> FundingRecordOutcome.Failed("verificationRequired")
    FundingFailure.VerificationRefused -> FundingRecordOutcome.Failed("verificationRefused")
    FundingFailure.BelowMinimum -> FundingRecordOutcome.Failed("belowMinimum")
    FundingFailure.AboveMaximum -> FundingRecordOutcome.Failed("aboveMaximum")
    FundingFailure.InsufficientBalance -> FundingRecordOutcome.Failed("insufficientBalance")
    FundingFailure.Expired -> FundingRecordOutcome.Failed("expired")
    FundingFailure.WrongAssetOrChain -> FundingRecordOutcome.Failed("wrongAssetOrChain")
    FundingFailure.ProviderTimeout -> FundingRecordOutcome.Failed("providerTimeout")
    FundingFailure.Cancelled -> FundingRecordOutcome.Failed("cancelled")
    is FundingFailure.Other -> FundingRecordOutcome.Failed(code)
}
