package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityItem
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.presentation.funding.FundingMoney
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityDayUi
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityRowUi
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingAmountTone
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingDayTitle
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingMoment
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowIcon
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowSubtitle
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowTitle
import kotlinx.collections.immutable.toImmutableList
import java.math.BigInteger
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.Locale
import kotlin.time.Instant
import kotlin.time.toJavaInstant

/**
 * Builds the card's funding lists, dated against [now] in [zone], amounts in [asset] under its [symbol].
 *
 * The session in flight drawn in front is a delayed one, then the newest. Each day of history carries the balance at
 * its end: the current balance less what every later day moved.
 */
class FundingActivityMapper(
    private val asset: Chain.Asset,
    private val symbol: String,
    private val now: Instant,
    private val zone: ZoneId = ZoneId.systemDefault(),
    private val locale: Locale = Locale.getDefault(),
) {
    private val timeFormat = DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT).withLocale(locale)
    private val dayFormat = DateTimeFormatter.ofPattern("d MMM", locale)
    private val dateFormat = DateTimeFormatter.ofPattern("d MMM yyyy", locale)
    private val dayTitleFormat = DateTimeFormatter.ofPattern("EEEE d MMMM", locale)
    private val dateTitleFormat = DateTimeFormatter.ofPattern("EEEE d MMMM yyyy", locale)
    private val today = localDate(now)

    fun map(activity: FundingActivity, currentBalance: Balance?): FundingActivityUiState {
        var later = BigInteger.ZERO
        val days = activity.history
            .groupBy { dayTitle(it.date) }
            .map { (title, items) ->
                val endOfDay = currentBalance?.let { Balance(it.value - later) }
                later += items.sumOf(::delta)
                FundingActivityDayUi(title, items.map(::row).toImmutableList(), endOfDay?.let(::label))
            }

        return FundingActivityUiState(
            inFlight = activity.inFlight
                .sortedWith(compareByDescending<FundingActivityItem> { it.isDelayed }.thenByDescending { it.date })
                .map(::row)
                .toImmutableList(),
            days = days.toImmutableList(),
            currentBalance = currentBalance?.let(::label),
        )
    }

    /** What an ended session changed the balance by: credited in, debited out less what came back. */
    private fun delta(item: FundingActivityItem): BigInteger {
        val amount = item.amount?.value ?: return BigInteger.ZERO
        return when (val status = item.status) {
            FundingActivityStatus.ToppedUp -> amount
            FundingActivityStatus.Sent -> -amount
            is FundingActivityStatus.PayoutFailed -> (status.refunded?.value ?: BigInteger.ZERO) - amount
            is FundingActivityStatus.Cancelled -> (status.refunded?.value ?: BigInteger.ZERO) - amount
            else -> BigInteger.ZERO
        }
    }

    private fun label(balance: Balance): String = FundingMoney.label(asset.amountFromPlanks(balance.atLeastZero()), symbol)

    private val FundingActivityItem.isDelayed: Boolean
        get() = (status as? FundingActivityStatus.InProgress)?.step?.isSlow == true

    private fun row(item: FundingActivityItem): FundingActivityRowUi {
        val status = item.status
        val inbound = item.direction == FundingDirection.IN
        val moved = status is FundingActivityStatus.InProgress || status == FundingActivityStatus.ToppedUp || status == FundingActivityStatus.Sent

        return FundingActivityRowUi(
            id = item.id,
            title = title(status, inbound),
            subtitle = when {
                status is FundingActivityStatus.InProgress -> FundingRowSubtitle.Step(status.step)
                status is FundingActivityStatus.Failed -> FundingRowSubtitle.Failed(status.reason)
                else -> FundingRowSubtitle.Ended(item.rail, moment(item.date))
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
            amount = item.amount?.let { balance ->
                val sign = if (!moved) "" else if (inbound) "+" else "-"
                sign + FundingMoney.label(asset.amountFromPlanks(balance), symbol)
            },
            amountTone = when (status) {
                FundingActivityStatus.ToppedUp -> FundingAmountTone.SUCCESS
                FundingActivityStatus.Sent -> FundingAmountTone.PRIMARY
                else -> FundingAmountTone.SECONDARY
            },
        )
    }

    private fun title(status: FundingActivityStatus, inbound: Boolean): FundingRowTitle = when (status) {
        is FundingActivityStatus.InProgress -> if (inbound) FundingRowTitle.TOPPING_UP else FundingRowTitle.SENDING
        FundingActivityStatus.ToppedUp -> FundingRowTitle.TOPPED_UP
        FundingActivityStatus.Sent -> FundingRowTitle.SENT
        FundingActivityStatus.Refunded -> FundingRowTitle.REFUNDED
        is FundingActivityStatus.PayoutFailed -> FundingRowTitle.PAYOUT_FAILED
        is FundingActivityStatus.Failed -> if (inbound) FundingRowTitle.TOP_UP_FAILED else FundingRowTitle.WITHDRAWAL_FAILED
        is FundingActivityStatus.Cancelled -> FundingRowTitle.CANCELLED
    }

    private fun dayTitle(date: Instant): FundingDayTitle {
        val day = localDate(date)
        return when {
            day == today -> FundingDayTitle.Today
            day == today.minusDays(1) -> FundingDayTitle.Yesterday
            day.year == today.year -> FundingDayTitle.Date(day.format(dayTitleFormat))
            else -> FundingDayTitle.Date(day.format(dateTitleFormat))
        }
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
