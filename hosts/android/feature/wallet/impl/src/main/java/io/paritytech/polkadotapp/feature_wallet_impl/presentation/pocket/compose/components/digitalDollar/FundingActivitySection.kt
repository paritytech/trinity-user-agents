package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowDropdown
import io.paritytech.polkadotapp.design.components.icon.vectors.CardReceived
import io.paritytech.polkadotapp.design.components.icon.vectors.CardSent
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_products_api.domain.funding.reasonText
import io.paritytech.polkadotapp.feature_products_api.presentation.funding.text
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityDayUi
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityRowUi
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingAmountTone
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingDayTitle
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingMoment
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowIcon
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowSubtitle
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowTitle
import kotlinx.collections.immutable.ImmutableList
import io.paritytech.polkadotapp.common.R as RCommon

private val ICON_SIZE = 48.dp
private val GLYPH_SIZE = 24.dp
private val SPINNER_SIZE = 24.dp
private val COUNT_SPINNER_SIZE = 32.dp
private val SPINNER_STROKE = 2.dp
private val DIVIDER_HEIGHT = 1.dp
private val CHEVRON_SIZE = 24.dp
private val HISTORY_ROW_HEIGHT = 76.dp
private val DOCK_ROW_HEIGHT = 84.dp
private val DOCK_HEADER_HEIGHT = 48.dp
private val DOCK_MAX_HEIGHT = 635.dp
private val DOCK_SHAPE = RoundedCornerShape(24.dp)
private const val TINT_ALPHA = 0.24f

/** The key of day [index]'s header in the history list, so the screen can tell which day has scrolled past. */
fun fundingDayKey(index: Int): String = "funding-day-$index"

/** The key of the history row for session [id]. */
fun fundingRowKey(id: String): String = "funding-row-$id"

/**
 * The ended sessions by day, newest first, as items of the screen's list: each day's header, then its rows with a
 * divider between them.
 */
fun LazyListScope.fundingHistory(
    days: ImmutableList<FundingActivityDayUi>,
    onRowClick: (id: String) -> Unit,
) {
    days.forEachIndexed { dayIndex, day ->
        item(key = fundingDayKey(dayIndex), contentType = "funding-day") {
            NovaText(
                modifier = Modifier.padding(
                    top = if (dayIndex == 0) 0.dp else PolkadotTheme.spacings.large,
                    bottom = PolkadotTheme.spacings.small,
                ),
                text = dayTitle(day.title),
                style = PolkadotTheme.typography.title.small,
                color = PolkadotTheme.colors.fg.primary,
            )
        }
        day.rows.forEachIndexed { index, row ->
            item(key = fundingRowKey(row.id), contentType = "funding-row") {
                Column {
                    if (index > 0) RowDivider()
                    FundingActivityRow(
                        modifier = Modifier
                            .height(HISTORY_ROW_HEIGHT)
                            .clickable { onRowClick(row.id) },
                        row = row,
                    )
                }
            }
        }
    }
}

/**
 * The sessions in flight, floating over the screen above the navigation bar: the front one only, a delayed one
 * before the newest. With more than one the spinner counts them and a tap opens the panel; with one a tap opens it.
 */
@Composable
fun FundingInProgressDock(
    rows: ImmutableList<FundingActivityRowUi>,
    onExpand: () -> Unit,
    onRowClick: (id: String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val front = rows.firstOrNull() ?: return
    val several = rows.size > 1

    PolkadotSurface(
        modifier = modifier.fillMaxWidth(),
        shape = DOCK_SHAPE,
        color = PolkadotTheme.colors.bg.surface.container,
        onClick = { if (several) onExpand() else onRowClick(front.id) },
    ) {
        FundingActivityRow(
            modifier = Modifier
                .height(DOCK_ROW_HEIGHT)
                .padding(horizontal = PolkadotTheme.spacings.medium),
            row = front,
            count = rows.size.takeIf { several },
        )
    }
}

/** Every session in flight, opened from the dock over a scrim; the header or the scrim folds it back. */
@Composable
fun FundingInProgressPanel(
    rows: ImmutableList<FundingActivityRowUi>,
    onCollapse: () -> Unit,
    onRowClick: (id: String) -> Unit,
    modifier: Modifier = Modifier,
) {
    PolkadotSurface(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(max = DOCK_MAX_HEIGHT),
        shape = DOCK_SHAPE,
        color = PolkadotTheme.colors.bg.surface.container,
    ) {
        Column {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(DOCK_HEADER_HEIGHT)
                    .clickable(onClick = onCollapse)
                    .padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                NovaText(
                    modifier = Modifier.weight(1f),
                    text = stringResource(RCommon.string.funding_activity_in_progress),
                    style = PolkadotTheme.typography.title.small,
                    color = PolkadotTheme.colors.fg.primary,
                )
                NovaIcon(modifier = Modifier.size(CHEVRON_SIZE), imageVector = NovaIcons.ArrowDropdown, tint = PolkadotTheme.colors.fg.primary)
            }

            LazyColumn {
                itemsIndexed(rows, key = { _, row -> row.id }) { index, row ->
                    Column(modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.medium)) {
                        if (index > 0) RowDivider()
                        FundingActivityRow(
                            modifier = Modifier
                                .height(DOCK_ROW_HEIGHT)
                                .clickable { onRowClick(row.id) },
                            row = row,
                        )
                    }
                }
            }
        }
    }
}

/** Behind the open panel: dims the screen, and a tap on it folds the panel back. */
@Composable
fun FundingInProgressScrim(onCollapse: () -> Unit, modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = SCRIM_ALPHA))
            .clickable(interactionSource = null, indication = null, onClick = onCollapse),
    )
}

private const val SCRIM_ALPHA = 0.48f

/** A 1dp rule that starts under the text column, as the rows' dividers do. */
@Composable
private fun RowDivider() {
    PolkadotSurface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = ICON_SIZE + PolkadotTheme.spacings.medium)
            .height(DIVIDER_HEIGHT),
        color = PolkadotTheme.colors.bg.surface.nested,
    ) {}
}

@Composable
private fun FundingActivityRow(row: FundingActivityRowUi, modifier: Modifier = Modifier, count: Int? = null) {
    val warning = row.icon == FundingRowIcon.IN_PROGRESS_WARNING

    Row(
        modifier = modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
    ) {
        RowIcon(row.icon, count)

        Column(modifier = Modifier.weight(1f)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.smallIncreased),
            ) {
                NovaText(
                    modifier = Modifier.weight(1f),
                    text = stringResource(row.title.textRes()),
                    style = PolkadotTheme.typography.title.small,
                    color = PolkadotTheme.colors.fg.primary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (row.amount != null) {
                    NovaText(
                        text = row.amount,
                        style = PolkadotTheme.typography.title.small,
                        color = when (row.amountTone) {
                            FundingAmountTone.SUCCESS -> PolkadotTheme.colors.fg.success
                            FundingAmountTone.PRIMARY -> PolkadotTheme.colors.fg.primary
                            FundingAmountTone.SECONDARY -> PolkadotTheme.colors.fg.secondary
                        },
                        maxLines = 1,
                    )
                }
            }
            VerticalSpacer { extraTiny }
            NovaText(
                text = subtitle(row.subtitle),
                style = PolkadotTheme.typography.body.small,
                color = if (warning) PolkadotTheme.colors.fg.warning else PolkadotTheme.colors.fg.secondary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun RowIcon(icon: FundingRowIcon, count: Int?) {
    val (background, tint) = when (icon) {
        FundingRowIcon.IN_PROGRESS -> PolkadotTheme.colors.bg.surface.nested to PolkadotTheme.colors.fg.primary
        FundingRowIcon.IN_PROGRESS_WARNING ->
            PolkadotTheme.colors.fg.warning.copy(alpha = TINT_ALPHA) to PolkadotTheme.colors.fg.warning
        FundingRowIcon.MOVED_IN -> PolkadotTheme.colors.bg.action.primary to PolkadotTheme.colors.fg.primaryInverted
        FundingRowIcon.MOVED_OUT -> PolkadotTheme.colors.bg.surface.nested to PolkadotTheme.colors.fg.primary
        FundingRowIcon.FAILED -> PolkadotTheme.colors.fg.error.copy(alpha = TINT_ALPHA) to PolkadotTheme.colors.fg.error
    }

    PolkadotSurface(
        modifier = Modifier.size(ICON_SIZE),
        shape = PolkadotTheme.shapes.full,
        color = background,
        contentAlignment = Alignment.Center,
    ) {
        when (icon) {
            FundingRowIcon.IN_PROGRESS, FundingRowIcon.IN_PROGRESS_WARNING -> if (count != null) {
                Box(contentAlignment = Alignment.Center) {
                    Spinner(size = COUNT_SPINNER_SIZE, tint = tint)
                    NovaText(text = count.toString(), style = PolkadotTheme.typography.body.mediumEmphasized, color = tint)
                }
            } else {
                Spinner(size = SPINNER_SIZE, tint = tint)
            }

            FundingRowIcon.MOVED_IN -> NovaIcon(modifier = Modifier.size(GLYPH_SIZE), imageVector = NovaIcons.CardReceived, tint = tint)
            FundingRowIcon.MOVED_OUT -> NovaIcon(modifier = Modifier.size(GLYPH_SIZE), imageVector = NovaIcons.CardSent, tint = tint)
            FundingRowIcon.FAILED -> NovaIcon(modifier = Modifier.size(GLYPH_SIZE), imageVector = NovaIcons.Close, tint = tint)
        }
    }
}

@Composable
private fun Spinner(size: Dp, tint: Color) {
    NovaCircularProgressIndicator(
        modifier = Modifier.size(size),
        color = tint,
        strokeWidth = SPINNER_STROKE,
    )
}

@Composable
private fun subtitle(subtitle: FundingRowSubtitle): String = when (subtitle) {
    is FundingRowSubtitle.Step -> when (val step = subtitle.step) {
        FundingActivityStep.WaitingForPayment -> stringResource(RCommon.string.funding_activity_waiting_payment)
        FundingActivityStep.WaitingForTransfer -> stringResource(RCommon.string.funding_activity_waiting_transfer)
        FundingActivityStep.WaitingForDeposit -> stringResource(RCommon.string.funding_activity_waiting_deposit)
        FundingActivityStep.Converting -> stringResource(RCommon.string.funding_activity_converting, LocalPaymentAssetBrand.current.symbol)
        FundingActivityStep.Retrying -> stringResource(RCommon.string.funding_activity_retrying)
        FundingActivityStep.TransactionInitiated -> stringResource(RCommon.string.funding_activity_initiated)
        FundingActivityStep.PreparingPayout -> stringResource(RCommon.string.funding_activity_preparing_payout)
        is FundingActivityStep.ConvertingOut -> stringResource(RCommon.string.funding_activity_converting_out, step.asset)
        FundingActivityStep.RetryingPayout -> stringResource(RCommon.string.funding_activity_retrying_payout)
        FundingActivityStep.TakingLonger -> stringResource(RCommon.string.funding_activity_delayed)
        FundingActivityStep.CancelRequested -> stringResource(RCommon.string.funding_activity_cancel_requested)
        FundingActivityStep.CancelOffered -> stringResource(RCommon.string.funding_activity_cancel_offered)
        is FundingActivityStep.CancelRefused -> stringResource(RCommon.string.funding_activity_cancel_refused)
        FundingActivityStep.Cancelling -> stringResource(RCommon.string.funding_activity_cancelling)
    }

    is FundingRowSubtitle.Failed -> subtitle.reason.reasonText.text()

    is FundingRowSubtitle.Ended -> {
        val moment = moment(subtitle.moment)
        if (subtitle.rail == null) moment else stringResource(RCommon.string.funding_activity_subtitle, stringResource(subtitle.rail.textRes()), moment)
    }
}

@Composable
private fun moment(moment: FundingMoment): String = when (moment) {
    is FundingMoment.TodayAt -> stringResource(RCommon.string.funding_date_today_at, moment.time)
    is FundingMoment.YesterdayAt -> stringResource(RCommon.string.funding_date_yesterday_at, moment.time)
    is FundingMoment.DayAt -> stringResource(RCommon.string.funding_date_day_at, moment.day, moment.time)
    is FundingMoment.Date -> moment.text
}

@Composable
internal fun dayTitle(title: FundingDayTitle): String = when (title) {
    FundingDayTitle.Today -> stringResource(RCommon.string.funding_date_today)
    FundingDayTitle.Yesterday -> stringResource(RCommon.string.funding_date_yesterday)
    is FundingDayTitle.Date -> title.text
}

private fun FundingRowTitle.textRes(): Int = when (this) {
    FundingRowTitle.TOPPING_UP -> RCommon.string.funding_activity_topping_up
    FundingRowTitle.SENDING -> RCommon.string.funding_activity_sending
    FundingRowTitle.TOPPED_UP -> RCommon.string.funding_activity_topped_up
    FundingRowTitle.SENT -> RCommon.string.funding_activity_sent
    FundingRowTitle.REFUNDED -> RCommon.string.funding_activity_refunded
    FundingRowTitle.PAYOUT_FAILED -> RCommon.string.funding_activity_payout_failed
    FundingRowTitle.TOP_UP_FAILED -> RCommon.string.funding_activity_top_up_failed
    FundingRowTitle.WITHDRAWAL_FAILED -> RCommon.string.funding_activity_withdrawal_failed
    FundingRowTitle.CANCELLED -> RCommon.string.funding_activity_cancelled
}

private fun FundingRailType.textRes(): Int = when (this) {
    FundingRailType.CARD -> RCommon.string.funding_rail_card
    FundingRailType.BANK -> RCommon.string.funding_rail_bank
    FundingRailType.CRYPTO -> RCommon.string.funding_rail_crypto
}
