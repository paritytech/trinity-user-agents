package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.withAmount
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityItem
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
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
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.Locale
import kotlin.time.Instant
import kotlin.time.toJavaInstant

/** Builds the card's funding lists, dated against [now] in [zone]. */
class FundingActivityMapper(
    private val asset: Chain.Asset,
    private val tokenAmountMapper: TokenAmountMapper,
    private val now: Instant,
    private val zone: ZoneId = ZoneId.systemDefault(),
    private val locale: Locale = Locale.getDefault(),
) {
    private val timeFormat = DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT).withLocale(locale)
    private val dayFormat = DateTimeFormatter.ofPattern("d MMM", locale)
    private val dateFormat = DateTimeFormatter.ofPattern("d MMM yyyy", locale)
    private val today = localDate(now)

    fun map(activity: FundingActivity): FundingActivityUiState = FundingActivityUiState(
        inFlight = activity.inFlight.map(::row).toImmutableList(),
        days = activity.history
            .groupBy { dayTitle(it.date) }
            .map { (title, items) -> FundingActivityDayUi(title, items.map(::row).toImmutableList()) }
            .toImmutableList(),
    )

    private fun row(item: FundingActivityItem): FundingActivityRowUi {
        val status = item.status
        val inbound = item.direction == FundingDirection.IN
        val moved = status is FundingActivityStatus.InProgress || status == FundingActivityStatus.ToppedUp || status == FundingActivityStatus.Sent

        return FundingActivityRowUi(
            id = item.id,
            title = title(status, inbound),
            subtitle = when (status) {
                is FundingActivityStatus.InProgress ->
                    if (status.delayed) FundingRowSubtitle.Delayed else FundingRowSubtitle.Step(status.step)

                else -> FundingRowSubtitle.Ended(item.rail, moment(item.date))
            },
            icon = when (status) {
                is FundingActivityStatus.InProgress ->
                    if (status.delayed || status.step == FundingActivityStep.Retrying) FundingRowIcon.IN_PROGRESS_WARNING else FundingRowIcon.IN_PROGRESS

                FundingActivityStatus.ToppedUp, FundingActivityStatus.Sent -> if (inbound) FundingRowIcon.MOVED_IN else FundingRowIcon.MOVED_OUT
                FundingActivityStatus.Refunded, FundingActivityStatus.PayoutFailed, FundingActivityStatus.Failed -> FundingRowIcon.FAILED
            },
            amount = item.amount?.let { tokenAmountMapper.mapFrom(asset.withAmount(it)) },
            amountSign = if (!moved) "" else if (inbound) "+" else "-",
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
        FundingActivityStatus.PayoutFailed -> FundingRowTitle.PAYOUT_FAILED
        FundingActivityStatus.Failed -> if (inbound) FundingRowTitle.TOP_UP_FAILED else FundingRowTitle.WITHDRAW_FAILED
    }

    private fun dayTitle(date: Instant): FundingDayTitle {
        val day = localDate(date)
        return when {
            day == today -> FundingDayTitle.Today
            day == today.minusDays(1) -> FundingDayTitle.Yesterday
            day.year == today.year -> FundingDayTitle.Date(day.format(dayFormat))
            else -> FundingDayTitle.Date(day.format(dateFormat))
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

    private fun localDate(instant: Instant): LocalDate = instant.toJavaInstant().atZone(zone).toLocalDate()
}
