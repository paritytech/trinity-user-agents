package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.ended

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCash
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.FundingRecord
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.FundingRecordOutcome
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.FundingRecordPayout
import kotlinx.collections.immutable.toImmutableList
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingRail
import io.paritytech.polkadotapp.common.R as RCommon

/** Builds an ended session's detail screen from the host's record of it, the same way iOS draws it. */
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
            steps = steps(record, outcome).toImmutableList(),
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

    private enum class Step { STARTED, PAYMENT, APPROVED, CONVERSION, ADDED, SENT, PAYOUT }

    private fun stepsFor(record: FundingRecord): List<Step> = when {
        record.direction == FundingDirection.OUT -> listOf(Step.STARTED, Step.PAYMENT, Step.SENT, Step.CONVERSION, Step.PAYOUT)
        record.rail == FundingRail.BANK -> listOf(Step.STARTED, Step.PAYMENT, Step.ADDED)
        else -> listOf(Step.STARTED, Step.PAYMENT, Step.APPROVED, Step.CONVERSION, Step.ADDED)
    }

    private fun steps(record: FundingRecord, outcome: Outcome): List<FundingEndedStep> {
        val steps = stepsFor(record)
        val pendingPayout = record.direction == FundingDirection.OUT &&
            record.outcome == FundingRecordOutcome.Released && record.payout == null
        if (pendingPayout) {
            val waitingAt = steps.indexOf(Step.PAYOUT)
            return steps.mapIndexed { index, step ->
                val state = when {
                    index < waitingAt -> FundingEndedStepState.DONE
                    index == waitingAt -> FundingEndedStepState.CURRENT
                    else -> FundingEndedStepState.UPCOMING
                }
                FundingEndedStep(title(step, record), state)
            }
        }
        val stoppedAt = when (outcome) {
            Outcome.Succeeded -> return emptyList()
            is Outcome.PayoutFailed -> steps.indexOf(Step.PAYOUT).takeIf { it >= 0 } ?: steps.lastIndex
            is Outcome.Failed -> steps.indexOf(Step.PAYMENT)
        }
        return steps.mapIndexed { index, step ->
            when {
                index < stoppedAt -> FundingEndedStep(title(step, record), FundingEndedStepState.DONE)
                index == stoppedAt -> FundingEndedStep(stoppedTitle(step, record, outcome), FundingEndedStepState.FAILED)
                else -> FundingEndedStep(title(step, record), FundingEndedStepState.UPCOMING)
            }
        }
    }

    private fun title(step: Step, record: FundingRecord): Int {
        val out = record.direction == FundingDirection.OUT
        return when (step) {
            Step.STARTED -> RCommon.string.funding_step_started
            Step.PAYMENT -> if (out) RCommon.string.funding_step_sending else RCommon.string.funding_step_payment
            Step.APPROVED -> RCommon.string.funding_step_approved
            Step.CONVERSION -> if (out) RCommon.string.funding_step_preparing else RCommon.string.funding_step_conversion
            Step.ADDED -> RCommon.string.funding_step_added
            Step.SENT -> RCommon.string.funding_step_received
            Step.PAYOUT -> RCommon.string.funding_step_payout
        }
    }

    private fun stoppedTitle(step: Step, record: FundingRecord, outcome: Outcome): Int = when (outcome) {
        Outcome.Succeeded -> title(step, record)
        is Outcome.PayoutFailed -> RCommon.string.funding_activity_payout_failed
        is Outcome.Failed -> when {
            outcome.reason == FundingFailureReason.Refunded -> RCommon.string.funding_activity_refunded
            step != Step.PAYMENT -> title(step, record)
            record.direction == FundingDirection.OUT -> RCommon.string.funding_ended_step_sending_failed
            outcome.reason == FundingFailureReason.Declined -> RCommon.string.funding_ended_step_payment_declined
            else -> RCommon.string.funding_ended_step_payment_failed
        }
    }

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
