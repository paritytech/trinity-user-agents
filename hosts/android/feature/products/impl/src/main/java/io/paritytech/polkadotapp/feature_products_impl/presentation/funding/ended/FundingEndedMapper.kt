package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.ended

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCash
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.toReason
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingAssetUnit
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCashFormat
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingTimeline
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingTimeline.milestone
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingTimelineUi
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.fundingNetworkName
import kotlinx.collections.immutable.toImmutableList
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingEndedOutcome
import uniffi.truapi.FundingPayoutState
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.FundingStage
import uniffi.truapi.U128
import kotlin.time.Instant
import io.paritytech.polkadotapp.common.R as RCommon

/** The dot a released payout still on its way sits on: between Received and Sent, once sending. */
private const val SENDING_DOT = 5

/**
 * Builds an ended session's detail screen from the core's summary of it. The core says how it went, what it moved and
 * where it stopped; the wording, the figures and the dots are the app's.
 */
class FundingEndedMapper(
    private val cash: FundingCash,
    private val format: FundingCashFormat,
) {
    /** `null` for a session that has not ended. */
    fun map(summary: FundingSessionSummary): FundingEndedUiState? {
        val ended = summary.ended
        val outcome = ended.outcome
        if (outcome is FundingEndedOutcome.Open) return null
        val succeeded = outcome is FundingEndedOutcome.Succeeded
        val inbound = summary.direction == FundingDirection.IN
        val amount = ended.cashAmount?.let(cash::decimal)

        return FundingEndedUiState(
            succeeded = succeeded,
            icon = if (!succeeded) FundingEndedIcon.FAILED else if (inbound) FundingEndedIcon.IN else FundingEndedIcon.OUT,
            figure = amount?.let { if (succeeded) (if (inbound) "+" else "-") + format.figure(it) else format.figure(it) },
            figureTone = when {
                !succeeded -> FundingEndedTone.TERTIARY
                inbound -> FundingEndedTone.SUCCESS
                else -> FundingEndedTone.PRIMARY
            },
            settledAt = summary.settledAt().takeIf { succeeded },
            timeline = timeline(summary, outcome),
            note = note(summary, outcome),
            rows = rows(summary, succeeded).toImmutableList(),
            action = if (succeeded) FundingEndedAction.CLOSE else FundingEndedAction.START_OVER,
        )
    }

    /** Every milestone done once it succeeded; a released payout still on its way sits on Sending; a failure is red where it stopped. */
    private fun timeline(summary: FundingSessionSummary, outcome: FundingEndedOutcome): FundingTimelineUi {
        val labels = FundingTimeline.labels(summary.direction)
        val stopped = summary.ended.stoppedAt?.milestone()
        return when {
            summary.ended.payoutState == FundingPayoutState.PENDING -> FundingTimeline.at(SENDING_DOT, failed = false, labels = labels)
            stopped == null -> FundingTimeline.at(FundingTimeline.DOTS, failed = false, labels = labels)
            else -> FundingTimeline.at(
                position = stopped * FundingTimeline.MILESTONE_GAP,
                failed = true,
                labels = labels.replacing(stopped, failedTitle(summary, outcome)),
            )
        }
    }

    private fun failedTitle(summary: FundingSessionSummary, outcome: FundingEndedOutcome): Int {
        val reason = (outcome as? FundingEndedOutcome.Failed)?.reason?.toReason()
        return when {
            outcome is FundingEndedOutcome.PayoutFailed -> RCommon.string.funding_activity_payout_failed
            reason == FundingFailureReason.Refunded -> RCommon.string.funding_activity_refunded
            summary.direction == FundingDirection.OUT -> RCommon.string.funding_ended_step_sending_failed
            reason == FundingFailureReason.Declined -> RCommon.string.funding_ended_step_payment_declined
            else -> RCommon.string.funding_ended_step_payment_failed
        }
    }

    private fun List<Int>.replacing(index: Int, label: Int): List<Int> = mapIndexed { i, it -> if (i == index) label else it }

    private fun note(summary: FundingSessionSummary, outcome: FundingEndedOutcome): FundingEndedText? = when (outcome) {
        FundingEndedOutcome.Open, FundingEndedOutcome.Succeeded -> null
        is FundingEndedOutcome.PayoutFailed -> when (val refunded = summary.refunded) {
            null -> if (summary.choice?.rail == FundingRail.CRYPTO) {
                res(RCommon.string.funding_ended_payout_failed_reason, outcome.reason)
            } else {
                res(RCommon.string.funding_ended_payout_failed)
            }
            else -> res(RCommon.string.funding_ended_withdraw_refunded, label(refunded))
        }
        is FundingEndedOutcome.Failed -> {
            val reason = outcome.reason.toReason()
            val refunded = summary.refunded
            when {
                reason == FundingFailureReason.Cancelled && refunded != null ->
                    res(RCommon.string.funding_activity_refunded_amount, label(refunded))
                summary.direction == FundingDirection.IN -> topUpNote(summary, reason)
                else -> withdrawNote(summary, reason)
            }
        }
    }

    private fun topUpNote(summary: FundingSessionSummary, reason: FundingFailureReason): FundingEndedText = when (reason) {
        FundingFailureReason.Declined -> when (summary.choice?.rail) {
            FundingRail.BANK -> res(RCommon.string.funding_ended_bank_declined)
            FundingRail.CARD -> res(RCommon.string.funding_ended_card_declined)
            else -> res(RCommon.string.funding_ended_top_up_failed)
        }
        FundingFailureReason.Refunded -> {
            val paid = summary.quotedAmount()
            when (summary.choice?.rail) {
                FundingRail.BANK -> paid?.let { res(RCommon.string.funding_ended_refunded_bank, it) }
                    ?: res(RCommon.string.funding_ended_refunded_bank_unknown)
                FundingRail.CRYPTO -> res(RCommon.string.funding_ended_refunded_crypto)
                else -> paid?.let { res(RCommon.string.funding_ended_refunded_card, it) }
                    ?: res(RCommon.string.funding_ended_refunded_card_unknown)
            }
        }
        FundingFailureReason.Cancelled, FundingFailureReason.Expired, is FundingFailureReason.Other ->
            res(RCommon.string.funding_ended_top_up_failed)
        else -> FundingEndedText.Reason(reason)
    }

    private fun withdrawNote(summary: FundingSessionSummary, reason: FundingFailureReason): FundingEndedText {
        val amount = summary.ended.cashAmount?.let(::label) ?: "$${format.symbol()}"
        return when (reason) {
            FundingFailureReason.Refunded -> res(RCommon.string.funding_ended_withdraw_refunded, amount)
            FundingFailureReason.Cancelled, FundingFailureReason.Expired, is FundingFailureReason.Other ->
                res(RCommon.string.funding_ended_withdraw_failed, amount)
            else -> FundingEndedText.Reason(reason)
        }
    }

    private fun rows(summary: FundingSessionSummary, succeeded: Boolean): List<FundingEndedRow> = buildList {
        val ended = summary.ended
        summary.quotedAmount()?.let { add(FundingEndedRow.Value(paidCaption(summary, succeeded), FundingEndedText.Plain(it))) }
        if (summary.settled()) {
            ended.cashAmount?.let { settled ->
                val caption = if (summary.direction == FundingDirection.IN) RCommon.string.funding_ended_credited else RCommon.string.funding_ended_debited
                add(FundingEndedRow.Settled(caption, label(settled), ended.differingRequested?.let(::label)))
            }
        }
        summary.choice?.rail?.let { rail ->
            val method = when (rail) {
                FundingRail.CARD -> res(RCommon.string.funding_rail_card)
                FundingRail.BANK -> res(RCommon.string.funding_ended_method_bank)
                FundingRail.CRYPTO -> summary.choice?.network?.takeIf { it.isNotEmpty() }
                    ?.let { res(RCommon.string.funding_ended_method_crypto_network, fundingNetworkName(it)) }
                    ?: res(RCommon.string.funding_rail_crypto)
            }
            add(FundingEndedRow.Value(RCommon.string.funding_ended_method, method))
        }
        summary.providerId?.let { add(FundingEndedRow.Provider(it)) }
        payout(ended.payoutState)?.let { add(FundingEndedRow.Value(RCommon.string.funding_ended_payout, res(it))) }
        summary.transactionId?.takeIf { it.isNotEmpty() }?.let { add(FundingEndedRow.Copy(RCommon.string.funding_ended_transaction_id, it)) }
        summary.reference?.takeIf { it.isNotEmpty() }?.let { add(FundingEndedRow.Copy(RCommon.string.funding_deposit_reference, it)) }
        add(FundingEndedRow.Moment(RCommon.string.funding_ended_started, Instant.fromEpochMilliseconds(summary.openedAtMs.toLong())))
        summary.settledAt()?.let {
            add(FundingEndedRow.Moment(if (succeeded) RCommon.string.funding_ended_completed else RCommon.string.funding_ended_ended, it))
        }
    }

    /** What the quoted amount was: paid or sent in, received out, or the amount of a withdrawal that failed. */
    private fun paidCaption(summary: FundingSessionSummary, succeeded: Boolean): Int = when {
        summary.direction == FundingDirection.IN && summary.choice?.rail == FundingRail.CARD -> RCommon.string.funding_ended_paid
        summary.direction == FundingDirection.IN -> RCommon.string.funding_ended_sent
        succeeded -> RCommon.string.funding_ended_received
        else -> RCommon.string.funding_ended_withdrawal_amount
    }

    private fun payout(state: FundingPayoutState?): Int? = when (state) {
        FundingPayoutState.PAID_OUT -> RCommon.string.funding_ended_payout_paid_out
        FundingPayoutState.FAILED -> RCommon.string.funding_ended_payout_failed_status
        FundingPayoutState.PENDING -> RCommon.string.funding_ended_payout_pending
        null -> null
    }

    private fun label(units: U128): String = format.label(cash.decimal(units))

    private fun res(res: Int, vararg args: String) = FundingEndedText.Res(res, args.toList())
}

/** Whether the balance moved: credited in, or debited out. */
private fun FundingSessionSummary.settled(): Boolean = stage is FundingStage.Delivered || stage is FundingStage.Released

/** When it ended. */
private fun FundingSessionSummary.settledAt(): Instant? = when (val stage = stage) {
    FundingStage.Open -> null
    is FundingStage.Delivered -> stage.settledAtMs
    is FundingStage.Released -> stage.settledAtMs
    is FundingStage.Failed -> stage.settledAtMs
}?.let { Instant.fromEpochMilliseconds(it.toLong()) }

/** What the user paid for value in, or received for value out, in the quote's asset at the decimals it states. */
private fun FundingSessionSummary.quotedAmount(): String? = choice?.let { choice ->
    val units = if (direction == FundingDirection.IN) choice.quote.sendAmount else choice.quote.receiveAmount
    FundingAssetUnit.of(choice.asset, choice.quote.assetDecimals.toInt()).format(units)
}
