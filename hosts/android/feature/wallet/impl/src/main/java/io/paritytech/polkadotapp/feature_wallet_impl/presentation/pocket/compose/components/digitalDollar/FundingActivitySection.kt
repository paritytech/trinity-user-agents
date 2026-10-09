package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.scale
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowDownward
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowUpward
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityRowUi
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingAmountTone
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingDayTitle
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingMoment
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowIcon
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowSubtitle
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowTitle
import io.paritytech.polkadotapp.common.R as RCommon

private val ICON_SIZE = 48.dp
private val GLYPH_SIZE = 20.dp
private val SPINNER_SIZE = 20.dp
private val SPINNER_STROKE = 2.dp
private val STACK_STEP = 10.dp
private const val STACK_DEPTH = 3
private const val STACK_SCALE_STEP = 0.04f
private const val STACK_BEHIND_ALPHA = 0.6f

/** What is in progress, stacked when there is more than one, then the history by day. */
@Composable
fun FundingActivitySection(
    state: FundingActivityUiState,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
    ) {
        if (state.inFlight.isNotEmpty()) {
            InProgress(rows = state.inFlight)
        }

        state.days.forEach { day ->
            Column {
                NovaText(
                    modifier = Modifier.padding(bottom = PolkadotTheme.spacings.small),
                    text = dayTitle(day.title),
                    style = PolkadotTheme.typography.title.small,
                    color = PolkadotTheme.colors.fg.primary,
                )
                day.rows.forEach { FundingActivityRow(row = it) }
            }
        }
    }
}

@Composable
private fun InProgress(rows: List<FundingActivityRowUi>) {
    var expanded by remember { mutableStateOf(false) }

    Column(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
        NovaText(
            text = stringResource(RCommon.string.funding_activity_in_progress, rows.size),
            style = PolkadotTheme.typography.title.small,
            color = PolkadotTheme.colors.fg.primary,
        )

        if (rows.size > 1 && !expanded) {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(bottom = STACK_STEP * (minOf(rows.size, STACK_DEPTH) - 1))
                    .clickable { expanded = true },
            ) {
                rows.take(STACK_DEPTH).withIndex().reversed().forEach { (index, row) ->
                    InProgressCard(
                        modifier = Modifier
                            .offset(y = STACK_STEP * index)
                            .scale(1f - STACK_SCALE_STEP * index)
                            .alpha(if (index == 0) 1f else STACK_BEHIND_ALPHA),
                        row = row,
                    )
                }
            }
        } else {
            rows.forEach { InProgressCard(row = it) }
        }
    }
}

@Composable
private fun InProgressCard(row: FundingActivityRowUi, modifier: Modifier = Modifier) {
    PolkadotSurface(
        modifier = modifier.fillMaxWidth(),
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.container,
    ) {
        FundingActivityRow(
            modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.medium),
            row = row,
        )
    }
}

@Composable
private fun FundingActivityRow(row: FundingActivityRowUi, modifier: Modifier = Modifier) {
    val formatter = LocalTokenAmountFormatter.current
    val warning = row.icon == FundingRowIcon.IN_PROGRESS_WARNING

    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = PolkadotTheme.spacings.smallIncreased),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.smallIncreased),
    ) {
        RowIcon(row.icon)

        Column(modifier = Modifier.weight(1f)) {
            NovaText(
                text = stringResource(row.title.textRes()),
                style = PolkadotTheme.typography.body.mediumEmphasized,
                color = PolkadotTheme.colors.fg.primary,
            )
            VerticalSpacer { extraTiny }
            NovaText(
                text = subtitle(row.subtitle),
                style = PolkadotTheme.typography.body.small,
                color = if (warning) PolkadotTheme.colors.fg.warning else PolkadotTheme.colors.fg.secondary,
            )
        }

        if (row.amount != null) {
            NovaText(
                text = row.amountSign + formatter.formatTokenAmount(row.amount, RoundPrecision.DEFAULT, withSymbol = true),
                style = PolkadotTheme.typography.body.mediumEmphasized,
                color = when (row.amountTone) {
                    FundingAmountTone.SUCCESS -> PolkadotTheme.colors.fg.success
                    FundingAmountTone.PRIMARY -> PolkadotTheme.colors.fg.primary
                    FundingAmountTone.SECONDARY -> PolkadotTheme.colors.fg.secondary
                },
            )
        }
    }
}

@Composable
private fun RowIcon(icon: FundingRowIcon) {
    val (background, tint) = when (icon) {
        FundingRowIcon.IN_PROGRESS -> PolkadotTheme.colors.bg.surface.nested to PolkadotTheme.colors.fg.primary
        FundingRowIcon.IN_PROGRESS_WARNING -> PolkadotTheme.colors.bg.status.warning to PolkadotTheme.colors.fg.warning
        FundingRowIcon.MOVED_IN -> PolkadotTheme.colors.bg.surface.containerInverted to PolkadotTheme.colors.fg.primaryInverted
        FundingRowIcon.MOVED_OUT -> PolkadotTheme.colors.bg.surface.nested to PolkadotTheme.colors.fg.primary
        FundingRowIcon.FAILED -> PolkadotTheme.colors.bg.status.error to PolkadotTheme.colors.fg.error
    }

    PolkadotSurface(
        modifier = Modifier.size(ICON_SIZE),
        shape = PolkadotTheme.shapes.full,
        color = background,
        contentAlignment = Alignment.Center,
    ) {
        when (icon) {
            FundingRowIcon.IN_PROGRESS, FundingRowIcon.IN_PROGRESS_WARNING -> NovaCircularProgressIndicator(
                modifier = Modifier.size(SPINNER_SIZE),
                color = tint,
                strokeWidth = SPINNER_STROKE,
            )

            FundingRowIcon.MOVED_IN -> NovaIcon(modifier = Modifier.size(GLYPH_SIZE), imageVector = NovaIcons.ArrowDownward, tint = tint)
            FundingRowIcon.MOVED_OUT -> NovaIcon(modifier = Modifier.size(GLYPH_SIZE), imageVector = NovaIcons.ArrowUpward, tint = tint)
            FundingRowIcon.FAILED -> NovaIcon(modifier = Modifier.size(GLYPH_SIZE), imageVector = NovaIcons.Close, tint = tint)
        }
    }
}

@Composable
private fun subtitle(subtitle: FundingRowSubtitle): String = when (subtitle) {
    FundingRowSubtitle.Delayed -> stringResource(RCommon.string.funding_activity_delayed)
    is FundingRowSubtitle.Step -> when (val step = subtitle.step) {
        FundingActivityStep.Upcoming -> stringResource(RCommon.string.funding_activity_upcoming)
        FundingActivityStep.WaitingForTransfer -> stringResource(RCommon.string.funding_activity_waiting_transfer)
        FundingActivityStep.Converting -> stringResource(RCommon.string.funding_activity_converting, LocalPaymentAssetBrand.current.symbol)
        FundingActivityStep.Retrying -> stringResource(RCommon.string.funding_activity_retrying)
        FundingActivityStep.TransactionInitiated -> stringResource(RCommon.string.funding_activity_initiated)
        is FundingActivityStep.ConvertingOut -> stringResource(RCommon.string.funding_activity_converting_out, step.asset)
    }

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
private fun dayTitle(title: FundingDayTitle): String = when (title) {
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
    FundingRowTitle.WITHDRAW_FAILED -> RCommon.string.funding_activity_withdraw_failed
}

private fun FundingRailType.textRes(): Int = when (this) {
    FundingRailType.CARD -> RCommon.string.funding_rail_card
    FundingRailType.BANK -> RCommon.string.funding_rail_bank
    FundingRailType.CRYPTO -> RCommon.string.funding_rail_crypto
}
