package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaModalBottomSheet
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowLeft
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowUpRight
import io.paritytech.polkadotapp.design.components.icon.vectors.Clock
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityRowUi
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingAmountTone
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingMoment
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowIcon
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowOutcome
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowSubtitle
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingSheet
import io.paritytech.polkadotapp.common.R as RCommon

private val ROW_SHAPE = RoundedCornerShape(16.dp)
private val ROW_MIN_HEIGHT = 56.dp
private val GLYPH_SIZE = 12.dp
private val SPINNER_STROKE = 1.5.dp
private val CROSS_STROKE = 1.6.dp
private val HEADER_BUTTON_SIZE = 32.dp
private val HEADER_ICON_SIZE = 20.dp
private val PILL_SHAPE = RoundedCornerShape(8.dp)
private const val OUT_ARROW_TURN = 180f

/** Hosts whichever funding sheet is shown; [onBack] leaves the history for the sheet it was opened from. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FundingActivitySheet(
    sheet: FundingSheet?,
    state: FundingActivityUiState,
    onHistory: () -> Unit,
    onBack: () -> Unit,
    onStartNew: () -> Unit,
    onRowClick: (id: String) -> Unit,
    onDismiss: () -> Unit,
) {
    NovaModalBottomSheet(isVisible = sheet != null, onDismissRequest = onDismiss) {
        when (sheet) {
            FundingSheet.HISTORY -> FundingHistorySheetContent(state = state, onBack = onBack, onRowClick = onRowClick)
            FundingSheet.TOPPING_UP, FundingSheet.SENDING -> FundingInProgressSheetContent(
                sheet = sheet,
                state = state,
                onHistory = onHistory,
                onStartNew = onStartNew,
                onRowClick = onRowClick,
            )
            null -> Unit
        }
    }
}

/**
 * The top-ups or withdrawals in progress, opened from Get or Send while one is: the rows, a clock that opens the
 * history, and a button that starts another.
 */
@Composable
private fun FundingInProgressSheetContent(
    sheet: FundingSheet,
    state: FundingActivityUiState,
    onHistory: () -> Unit,
    onStartNew: () -> Unit,
    onRowClick: (id: String) -> Unit,
) {
    val sending = sheet == FundingSheet.SENDING
    val rows = if (sending) state.sending else state.toppingUp
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.mediumIncreased),
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            NovaText(
                modifier = Modifier.weight(1f),
                text = stringResource(if (sending) RCommon.string.funding_activity_sending else RCommon.string.funding_activity_topping_up),
                style = PolkadotTheme.typography.title.small,
                color = PolkadotTheme.colors.fg.primary,
            )
            HeaderButton(icon = NovaIcons.Clock, onClick = onHistory)
        }
        LazyColumn(
            modifier = Modifier.weight(1f, fill = false),
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
        ) {
            items(rows, key = { it.id }) { row -> FundingActivityRow(row = row, onClick = { onRowClick(row.id) }) }
        }
        VerticalSpacer { medium }
        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(if (sending) RCommon.string.funding_send_money_out else RCommon.string.funding_new_top_up),
            onClick = onStartNew,
        )
    }
}

/** Top-ups and withdrawals together: those in progress, then those completed, newest first. */
@Composable
private fun FundingHistorySheetContent(
    state: FundingActivityUiState,
    onBack: () -> Unit,
    onRowClick: (id: String) -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.mediumIncreased),
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
    ) {
        Box(modifier = Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
            Box(modifier = Modifier.align(Alignment.CenterStart)) { HeaderButton(icon = NovaIcons.ArrowLeft, onClick = onBack) }
            NovaText(
                text = stringResource(RCommon.string.funding_history_title),
                style = PolkadotTheme.typography.title.small,
                color = PolkadotTheme.colors.fg.primary,
            )
        }
        if (state.inFlight.isEmpty() && state.completed.isEmpty()) {
            NovaText(
                modifier = Modifier.padding(vertical = PolkadotTheme.spacings.large),
                text = stringResource(RCommon.string.funding_history_empty),
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.secondary,
            )
        }
        LazyColumn(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
            section(RCommon.string.funding_activity_in_progress, state.inFlight, onRowClick)
            section(RCommon.string.funding_activity_completed, state.completed, onRowClick)
        }
    }
}

private fun LazyListScope.section(
    title: Int,
    rows: List<FundingActivityRowUi>,
    onRowClick: (id: String) -> Unit,
) {
    if (rows.isEmpty()) return
    item(key = "section-$title", contentType = "section") {
        NovaText(
            modifier = Modifier.padding(top = PolkadotTheme.spacings.medium, bottom = PolkadotTheme.spacings.extraSmall),
            text = stringResource(title),
            style = PolkadotTheme.typography.title.small,
            color = PolkadotTheme.colors.fg.primary,
        )
    }
    items(rows, key = { it.id }, contentType = { "row" }) { row -> FundingActivityRow(row = row, onClick = { onRowClick(row.id) }) }
}

@Composable
private fun HeaderButton(icon: ImageVector, onClick: () -> Unit) {
    PolkadotSurface(
        modifier = Modifier.size(HEADER_BUTTON_SIZE),
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.surface.nested,
        contentAlignment = Alignment.Center,
        onClick = onClick,
    ) {
        NovaIcon(modifier = Modifier.size(HEADER_ICON_SIZE), imageVector = icon, tint = PolkadotTheme.colors.fg.primary)
    }
}

/** One session: a glyph and the amount, the rail in a pill, and the line under it. */
@Composable
fun FundingActivityRow(row: FundingActivityRowUi, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val warning = row.icon == FundingRowIcon.IN_PROGRESS_WARNING
    PolkadotSurface(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = ROW_MIN_HEIGHT),
        shape = ROW_SHAPE,
        color = PolkadotTheme.colors.bg.surface.container,
        onClick = onClick,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.medium, vertical = PolkadotTheme.spacings.smallIncreased),
            verticalAlignment = Alignment.Top,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
                ) {
                    RowGlyph(row.icon)
                    NovaText(
                        text = row.amount.orEmpty(),
                        style = PolkadotTheme.typography.body.mediumEmphasized,
                        color = when (row.amountTone) {
                            FundingAmountTone.SUCCESS -> PolkadotTheme.colors.fg.success
                            FundingAmountTone.PRIMARY -> PolkadotTheme.colors.fg.primary
                            FundingAmountTone.SECONDARY -> PolkadotTheme.colors.fg.secondary
                        },
                        maxLines = 1,
                    )
                }
                NovaText(
                    text = subtitle(row.subtitle),
                    style = PolkadotTheme.typography.body.small,
                    color = if (warning) PolkadotTheme.colors.fg.warning else PolkadotTheme.colors.fg.secondary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            row.rail?.let { RailPill(it) }
        }
    }
}

@Composable
private fun RailPill(rail: FundingRailType) {
    PolkadotSurface(shape = PILL_SHAPE, color = PolkadotTheme.colors.bg.surface.nested) {
        NovaText(
            modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.small, vertical = PolkadotTheme.spacings.extraTiny),
            text = stringResource(rail.textRes()),
            style = PolkadotTheme.typography.body.small,
            color = PolkadotTheme.colors.fg.secondary,
        )
    }
}

@Composable
private fun RowGlyph(icon: FundingRowIcon) {
    when (icon) {
        FundingRowIcon.IN_PROGRESS, FundingRowIcon.IN_PROGRESS_WARNING -> NovaCircularProgressIndicator(
            modifier = Modifier.size(GLYPH_SIZE),
            color = if (icon == FundingRowIcon.IN_PROGRESS_WARNING) PolkadotTheme.colors.fg.warning else PolkadotTheme.colors.fg.primary,
            strokeWidth = SPINNER_STROKE,
        )

        FundingRowIcon.MOVED_IN, FundingRowIcon.MOVED_OUT -> NovaIcon(
            modifier = Modifier
                .size(GLYPH_SIZE)
                .graphicsLayer { if (icon == FundingRowIcon.MOVED_OUT) rotationZ = OUT_ARROW_TURN },
            imageVector = NovaIcons.ArrowUpRight,
            tint = if (icon == FundingRowIcon.MOVED_IN) PolkadotTheme.colors.fg.success else PolkadotTheme.colors.fg.secondary,
        )

        FundingRowIcon.FAILED -> Cross(tint = PolkadotTheme.colors.fg.error)
    }
}

@Composable
private fun Cross(tint: Color) {
    Canvas(modifier = Modifier.size(GLYPH_SIZE)) {
        val stroke = CROSS_STROKE.toPx()
        drawLine(tint, Offset(0f, 0f), Offset(size.width, size.height), stroke, StrokeCap.Round)
        drawLine(tint, Offset(size.width, 0f), Offset(0f, size.height), stroke, StrokeCap.Round)
    }
}

@Composable
private fun subtitle(subtitle: FundingRowSubtitle): String = when (subtitle) {
    is FundingRowSubtitle.Step -> when (val step = subtitle.step) {
        FundingActivityStep.WaitingForPayment -> stringResource(RCommon.string.funding_activity_waiting_payment)
        FundingActivityStep.WaitingForTransfer -> stringResource(RCommon.string.funding_activity_waiting_transfer)
        FundingActivityStep.WaitingForDeposit -> stringResource(RCommon.string.funding_activity_waiting_deposit)
        FundingActivityStep.Converting -> stringResource(RCommon.string.funding_activity_converting)
        FundingActivityStep.AddingToBalance -> stringResource(RCommon.string.funding_activity_adding)
        FundingActivityStep.Retrying -> stringResource(RCommon.string.funding_activity_retrying)
        FundingActivityStep.TransactionInitiated -> stringResource(RCommon.string.funding_activity_initiated)
        FundingActivityStep.PreparingPayout -> stringResource(RCommon.string.funding_activity_preparing_payout)
        is FundingActivityStep.ConvertingOut -> stringResource(RCommon.string.funding_activity_converting_out, step.asset)
        FundingActivityStep.RetryingPayout -> stringResource(RCommon.string.funding_activity_retrying_payout)
        FundingActivityStep.TakingLonger -> stringResource(
            if (subtitle.inbound) RCommon.string.funding_activity_delayed else RCommon.string.funding_activity_delayed_out,
        )
        FundingActivityStep.CancelRequested -> stringResource(RCommon.string.funding_activity_cancel_requested)
        FundingActivityStep.CancelOffered -> stringResource(RCommon.string.funding_activity_cancel_offered)
        is FundingActivityStep.CancelRefused -> stringResource(RCommon.string.funding_activity_cancel_refused)
        FundingActivityStep.Cancelling -> stringResource(RCommon.string.funding_activity_cancelling)
    }

    is FundingRowSubtitle.Ended -> {
        val moment = moment(subtitle.moment)
        when (subtitle.outcome) {
            FundingRowOutcome.MOVED -> moment
            FundingRowOutcome.REFUNDED -> outcomeAt(RCommon.string.funding_activity_refunded, moment)
            FundingRowOutcome.PAYOUT_FAILED -> outcomeAt(RCommon.string.funding_activity_payout_failed, moment)
            FundingRowOutcome.EXPIRED -> outcomeAt(RCommon.string.funding_activity_expired, moment)
            FundingRowOutcome.PAYMENT_FAILED -> outcomeAt(RCommon.string.funding_activity_payment_failed, moment)
            FundingRowOutcome.CANCELLED -> outcomeAt(RCommon.string.funding_activity_cancelled, moment)
        }
    }
}

@Composable
private fun outcomeAt(outcome: Int, moment: String): String =
    stringResource(RCommon.string.funding_activity_subtitle, stringResource(outcome), moment)

@Composable
private fun moment(moment: FundingMoment): String = when (moment) {
    is FundingMoment.TodayAt -> stringResource(RCommon.string.funding_date_today_at, moment.time)
    is FundingMoment.YesterdayAt -> stringResource(RCommon.string.funding_date_yesterday_at, moment.time)
    is FundingMoment.DayAt -> stringResource(RCommon.string.funding_date_day_at, moment.day, moment.time)
    is FundingMoment.Date -> moment.text
}

private fun FundingRailType.textRes(): Int = when (this) {
    FundingRailType.CARD -> RCommon.string.funding_rail_card
    FundingRailType.BANK -> RCommon.string.funding_rail_bank
    FundingRailType.CRYPTO -> RCommon.string.funding_rail_crypto
}
