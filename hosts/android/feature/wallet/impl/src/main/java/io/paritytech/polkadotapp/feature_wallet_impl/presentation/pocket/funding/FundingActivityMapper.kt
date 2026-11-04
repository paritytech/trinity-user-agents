package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityItem
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_api.presentation.funding.FundingMoney
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityRowUi
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingAmountTone
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingMoment
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowIcon
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowOutcome
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowSubtitle
import kotlinx.collections.immutable.toImmutableList
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.Locale
import kotlin.time.Instant
import kotlin.time.toJavaInstant
import android.text.format.DateFormat as AndroidDateFormat

/**
 * Builds the CASH card's funding lists, dated against [now] in [zone], amounts in [asset] under its [symbol]. A
 * day this year reads by [dayPattern], the locale's own month-and-day.
 *
 * In progress, a delayed session comes first, then the newest; completed ones are newest first.
 */
class FundingActivityMapper(
    private val asset: Chain.Asset,
    private val symbol: String,
    private val now: Instant,
    private val zone: ZoneId = ZoneId.systemDefault(),
    private val locale: Locale = Locale.getDefault(),
    dayPattern: String = AndroidDateFormat.getBestDateTimePattern(locale, "MMMd"),
) {
    private val timeFormat = DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT).withLocale(locale)
    private val dayFormat = DateTimeFormatter.ofPattern(dayPattern, locale)
    private val dateFormat = DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM).withLocale(locale)
    private val today = localDate(now)

    fun map(activity: FundingActivity): FundingActivityUiState = FundingActivityUiState(
        inFlight = activity.inFlight
            .sortedWith(compareByDescending<FundingActivityItem> { it.isDelayed }.thenByDescending { it.date })
            .map(::row)
            .toImmutableList(),
        completed = activity.history.map(::row).toImmutableList(),
    )

    private val FundingActivityItem.isDelayed: Boolean
        get() = (status as? FundingActivityStatus.InProgress)?.step?.isSlow == true

    private fun row(item: FundingActivityItem): FundingActivityRowUi {
        val status = item.status
        val inbound = item.direction == FundingDirection.IN
        val moved = status == FundingActivityStatus.ToppedUp || status == FundingActivityStatus.Sent
        val sign = when {
            moved -> if (inbound) "+" else "-"
            status is FundingActivityStatus.InProgress && !inbound -> "-"
            else -> ""
        }

        return FundingActivityRowUi(
            id = item.id,
            inbound = inbound,
            amount = item.amount?.let { sign + FundingMoney.label(asset.amountFromPlanks(it), symbol) },
            amountTone = when (status) {
                FundingActivityStatus.ToppedUp -> FundingAmountTone.SUCCESS
                FundingActivityStatus.Sent, is FundingActivityStatus.InProgress -> FundingAmountTone.PRIMARY
                else -> FundingAmountTone.SECONDARY
            },
            rail = item.rail,
            subtitle = when (status) {
                is FundingActivityStatus.InProgress -> FundingRowSubtitle.Step(status.step, inbound)
                FundingActivityStatus.ToppedUp, FundingActivityStatus.Sent -> ended(FundingRowOutcome.MOVED, item)
                FundingActivityStatus.Refunded -> ended(FundingRowOutcome.REFUNDED, item)
                is FundingActivityStatus.PayoutFailed -> ended(FundingRowOutcome.PAYOUT_FAILED, item)
                is FundingActivityStatus.Cancelled -> ended(FundingRowOutcome.CANCELLED, item)
                is FundingActivityStatus.Failed -> ended(failedOutcome(status.reason), item)
            },
            icon = when (status) {
                is FundingActivityStatus.InProgress ->
                    if (status.step.isSlow) FundingRowIcon.IN_PROGRESS_WARNING else FundingRowIcon.IN_PROGRESS

                FundingActivityStatus.ToppedUp, FundingActivityStatus.Sent -> if (inbound) FundingRowIcon.MOVED_IN else FundingRowIcon.MOVED_OUT
                FundingActivityStatus.Refunded,
                is FundingActivityStatus.PayoutFailed,
                is FundingActivityStatus.Failed,
                is FundingActivityStatus.Cancelled,
                -> FundingRowIcon.FAILED
            },
        )
    }

    private fun ended(outcome: FundingRowOutcome, item: FundingActivityItem) = FundingRowSubtitle.Ended(outcome, moment(item.date))

    private fun failedOutcome(reason: FundingFailureReason): FundingRowOutcome = when (reason) {
        FundingFailureReason.Expired -> FundingRowOutcome.EXPIRED
        FundingFailureReason.Cancelled -> FundingRowOutcome.CANCELLED
        FundingFailureReason.Refunded -> FundingRowOutcome.REFUNDED
        else -> FundingRowOutcome.PAYMENT_FAILED
    }

    private fun moment(date: Instant): FundingMoment {
        val local = date.toJavaInstant().atZone(zone)
        val day = local.toLocalDate()
        val time = local.format(timeFormat)
        return when {
            day == today -> FundingMoment.TodayAt(time)
            day == today.minusDays(1) -> FundingMoment.YesterdayAt(time)
            day.year == today.year -> FundingMoment.DayAt(day.format(dayFormat), time)
            else -> FundingMoment.Date(day.format(dateFormat))
        }
    }

    private val FundingActivityStep.isSlow: Boolean
        get() = this == FundingActivityStep.Retrying ||
            this == FundingActivityStep.RetryingPayout ||
            this == FundingActivityStep.TakingLonger ||
            this is FundingActivityStep.CancelRefused

    private fun localDate(instant: Instant): LocalDate = instant.toJavaInstant().atZone(zone).toLocalDate()
}
