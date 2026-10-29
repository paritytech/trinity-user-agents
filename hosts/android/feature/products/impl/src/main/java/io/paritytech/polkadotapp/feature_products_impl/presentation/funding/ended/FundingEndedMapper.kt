package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.ended

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCash
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.FundingRecord
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.FundingRecordOutcome
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.FundingRecordPayout
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingTimeline
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingTimelineUi
import kotlinx.collections.immutable.toImmutableList
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingRail
import io.paritytech.polkadotapp.common.R as RCommon

/** Builds an ended session's detail screen from the host's record of it, the same way iOS draws it. */
private const val MIDDLE_MILESTONE = 1
private const val LAST_MILESTONE = 2
private const val SENDING_DOT = 5
private const val LAST_MILESTONE_DOT = 6

class FundingEndedMapper(private val cash: FundingCash) {
    fun map(record: FundingRecord): FundingEndedUiState {
        val outcome = outcome(record)
        val succeeded = outcome is Outcome.Succeeded
        val inbound = record.direction == FundingDirection.IN
        val amount = (record.settledAmount ?: record.requestedAmount)?.let(::decimal)

        return FundingEndedUiState(
            succeeded = succeeded,
            icon = if (!succeeded) FundingEndedIcon.FAILED else if (inbound) FundingEndedIcon.IN else FundingEndedIcon.OUT,
            figure = amount?.let { if (succeeded) (if (inbound) "+" else "-") + cash.figure(it) else cash.figure(it) },
            figureTone = when {
                !succeeded -> FundingEndedTone.TERTIARY
                inbound -> FundingEndedTone.SUCCESS
                else -> FundingEndedTone.PRIMARY
            },
            settledAt = record.settledAt.takeIf { succeeded },
            timeline = timeline(record, outcome),
            note = note(record, outcome),
            rows = rows(record, succeeded).toImmutableList(),
            action = if (succeeded) FundingEndedAction.CLOSE else FundingEndedAction.START_OVER,
        )
    }

    private sealed interface Outcome {
        data object Succeeded : Outcome

        data class Failed(val reason: FundingFailureReason) : Outcome

        data class PayoutFailed(val reason: String) : Outcome
    }

    private fun outcome(record: FundingRecord): Outcome = when (val outcome = record.outcome) {
        FundingRecordOutcome.Delivered -> Outcome.Succeeded
        FundingRecordOutcome.Released -> (record.payout as? FundingRecordPayout.Failed)?.let { Outcome.PayoutFailed(it.reason) } ?: Outcome.Succeeded
        FundingRecordOutcome.Refunded -> Outcome.Failed(FundingFailureReason.Refunded)
        FundingRecordOutcome.Cancelled -> Outcome.Failed(FundingFailureReason.Cancelled)
        is FundingRecordOutcome.Failed -> Outcome.Failed(outcome.reason)
    }

    /** Every milestone done once it succeeded; a released payout still on its way sits on Sending; a failure is red where it stopped. */
    private fun timeline(record: FundingRecord, outcome: Outcome): FundingTimelineUi {
        val labels = FundingTimeline.labels(record.direction)
        val pendingPayout = record.direction == FundingDirection.OUT &&
            record.outcome == FundingRecordOutcome.Released && record.payout == null
        return when {
            pendingPayout -> FundingTimeline.at(SENDING_DOT, failed = false, labels = labels)
            outcome is Outcome.Succeeded -> FundingTimeline.at(FundingTimeline.DOTS, failed = false, labels = labels)
            outcome is Outcome.PayoutFailed -> FundingTimeline.at(
                position = LAST_MILESTONE_DOT,
                failed = true,
                labels = labels.replacing(LAST_MILESTONE, RCommon.string.funding_activity_payout_failed),
            )
            else -> FundingTimeline.at(
                position = FundingTimeline.MILESTONE_GAP,
                failed = true,
                labels = labels.replacing(MIDDLE_MILESTONE, failedTitle(record, (outcome as Outcome.Failed).reason)),
            )
        }
    }

    private fun failedTitle(record: FundingRecord, reason: FundingFailureReason): Int = when {
        reason == FundingFailureReason.Refunded -> RCommon.string.funding_activity_refunded
        record.direction == FundingDirection.OUT -> RCommon.string.funding_ended_step_sending_failed
        reason == FundingFailureReason.Declined -> RCommon.string.funding_ended_step_payment_declined
        else -> RCommon.string.funding_ended_step_payment_failed
    }

    private fun List<Int>.replacing(index: Int, label: Int): List<Int> = mapIndexed { i, it -> if (i == index) label else it }

    private fun note(record: FundingRecord, outcome: Outcome): FundingEndedText? = when (outcome) {
        Outcome.Succeeded -> null
        is Outcome.PayoutFailed -> when {
            record.refunded != null -> res(RCommon.string.funding_ended_withdraw_refunded, label(record.refunded))
            record.rail == FundingRail.CRYPTO -> res(RCommon.string.funding_ended_payout_failed_reason, outcome.reason)
            else -> res(RCommon.string.funding_ended_payout_failed)
        }
        is Outcome.Failed -> when {
            outcome.reason == FundingFailureReason.Cancelled && record.refunded != null ->
                res(RCommon.string.funding_activity_refunded_amount, label(record.refunded))
            record.direction == FundingDirection.IN -> topUpNote(record, outcome.reason)
            else -> withdrawNote(record, outcome.reason)
        }
    }

    private fun topUpNote(record: FundingRecord, reason: FundingFailureReason): FundingEndedText = when (reason) {
        FundingFailureReason.Declined -> when (record.rail) {
            FundingRail.BANK -> res(RCommon.string.funding_ended_bank_declined)
            FundingRail.CARD -> res(RCommon.string.funding_ended_card_declined)
            else -> res(RCommon.string.funding_ended_top_up_failed)
        }
        FundingFailureReason.Refunded -> when (record.rail) {
            FundingRail.BANK -> res(RCommon.string.funding_ended_refunded_bank_unknown)
            FundingRail.CRYPTO -> res(RCommon.string.funding_ended_refunded_crypto)
            else -> res(RCommon.string.funding_ended_refunded_card_unknown)
        }
        FundingFailureReason.Cancelled, FundingFailureReason.Expired, is FundingFailureReason.Other ->
            res(RCommon.string.funding_ended_top_up_failed)
        else -> FundingEndedText.Reason(reason)
    }

    private fun withdrawNote(record: FundingRecord, reason: FundingFailureReason): FundingEndedText {
        val amount = (record.settledAmount ?: record.requestedAmount)?.let(::label) ?: "$${cash.symbol}"
        return when (reason) {
            FundingFailureReason.Refunded -> res(RCommon.string.funding_ended_withdraw_refunded, amount)
            FundingFailureReason.Cancelled, FundingFailureReason.Expired, is FundingFailureReason.Other ->
                res(RCommon.string.funding_ended_withdraw_failed, amount)
            else -> FundingEndedText.Reason(reason)
        }
    }

    private fun rows(record: FundingRecord, succeeded: Boolean): List<FundingEndedRow> = buildList {
        record.settledAmount?.let { settled ->
            val requested = record.requestedAmount?.takeIf { it != settled }?.let(::label)
            val caption = if (record.direction == FundingDirection.IN) RCommon.string.funding_ended_credited else RCommon.string.funding_ended_debited
            add(FundingEndedRow.Settled(caption, label(settled), requested))
        }
        record.rail?.let { rail ->
            val method = when (rail) {
                FundingRail.CARD -> res(RCommon.string.funding_rail_card)
                FundingRail.BANK -> res(RCommon.string.funding_ended_method_bank)
                FundingRail.CRYPTO -> res(RCommon.string.funding_rail_crypto)
            }
            add(FundingEndedRow.Value(RCommon.string.funding_ended_method, method))
        }
        record.providerId?.let { add(FundingEndedRow.Provider(it)) }
        payout(record)?.let { add(FundingEndedRow.Value(RCommon.string.funding_ended_payout, res(it))) }
        record.transactionId?.takeIf { it.isNotEmpty() }?.let { add(FundingEndedRow.Copy(RCommon.string.funding_ended_transaction_id, it)) }
        record.reference?.takeIf { it.isNotEmpty() }?.let { add(FundingEndedRow.Copy(RCommon.string.funding_deposit_reference, it)) }
        add(FundingEndedRow.Moment(RCommon.string.funding_ended_started, record.openedAt))
        add(FundingEndedRow.Moment(if (succeeded) RCommon.string.funding_ended_completed else RCommon.string.funding_ended_ended, record.settledAt))
    }

    private fun payout(record: FundingRecord): Int? {
        if (record.direction != FundingDirection.OUT || record.outcome != FundingRecordOutcome.Released) return null
        return when (record.payout) {
            FundingRecordPayout.PaidOut -> RCommon.string.funding_ended_payout_paid_out
            is FundingRecordPayout.Failed -> RCommon.string.funding_ended_payout_failed_status
            null -> RCommon.string.funding_ended_payout_pending
        }
    }

    private fun decimal(balance: Balance) = cash.decimal(balance.value.toString())

    private fun label(balance: Balance): String = cash.label(decimal(balance))

    private fun res(res: Int, vararg args: String) = FundingEndedText.Res(res, args.toList())
}
