package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowDropdown
import io.paritytech.polkadotapp.design.components.icon.vectors.CardReceived
import io.paritytech.polkadotapp.design.components.icon.vectors.CardSent
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_products_api.domain.funding.reasonText
import io.paritytech.polkadotapp.feature_products_api.presentation.funding.text
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation.PocketMotion
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation.rememberReducedMotion
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityDayUi
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityRowUi
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingAmountTone
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingDayTitle
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingMoment
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowIcon
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowSubtitle
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowTitle
import kotlinx.collections.immutable.ImmutableList
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
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
private val CROSS_SIZE = 14.dp
private val CROSS_STROKE = 2.4.dp
private const val SPINNER_OUT_SPEED = 2f
private const val SPINNER_OUT_SHRINK = 0.2f
private const val GLYPH_START_SCALE = 0.42f
private const val CHEVRON_UP = 180f

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
    arrived: Set<String>,
    onArrived: (id: String) -> Unit,
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
                val arriving = row.id in arrived
                ArrivingRow(arriving = arriving, onArrived = { onArrived(row.id) }) {
                    if (index > 0) RowDivider()
                    FundingActivityRow(
                        modifier = Modifier
                            .height(HISTORY_ROW_HEIGHT)
                            .clickable { onRowClick(row.id) },
                        row = row,
                        animateIcon = arriving,
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
    // The last rows stay on the dock while it slides away.
    var shown by remember { mutableStateOf(rows) }
    if (rows.isNotEmpty()) shown = rows
    val reduced = rememberReducedMotion()

    AnimatedVisibility(
        modifier = modifier,
        visible = rows.isNotEmpty(),
        enter = if (reduced) EnterTransition.None else fadeIn(tween(PocketMotion.DOCK_EXIT_MILLIS)) +
            slideInVertically(tween(PocketMotion.DOCK_EXIT_MILLIS, easing = PocketMotion.Standard)) { it / 6 },
        exit = if (reduced) ExitTransition.None else fadeOut(tween(PocketMotion.DOCK_EXIT_MILLIS)) +
            slideOutVertically(tween(PocketMotion.DOCK_EXIT_MILLIS, easing = PocketMotion.Standard)) { it / 6 },
    ) {
        val front = shown.firstOrNull()
        if (front != null) {
            val several = shown.size > 1
            PolkadotSurface(
                modifier = Modifier.fillMaxWidth(),
                shape = DOCK_SHAPE,
                color = PolkadotTheme.colors.bg.surface.container,
                onClick = { if (several) onExpand() else onRowClick(front.id) },
            ) {
                FundingActivityRow(
                    modifier = Modifier
                        .height(DOCK_ROW_HEIGHT)
                        .padding(horizontal = PolkadotTheme.spacings.medium),
                    row = front,
                    count = shown.size.takeIf { several },
                )
            }
        }
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
private fun FundingActivityRow(
    row: FundingActivityRowUi,
    modifier: Modifier = Modifier,
    count: Int? = null,
    animateIcon: Boolean = false,
) {
    val warning = row.icon == FundingRowIcon.IN_PROGRESS_WARNING

    Row(
        modifier = modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
    ) {
        RowIcon(row.icon, count, animateIcon)

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
private fun RowIcon(icon: FundingRowIcon, count: Int?, animate: Boolean) {
    val (background, tint) = when (icon) {
        FundingRowIcon.IN_PROGRESS -> PolkadotTheme.colors.bg.surface.nested to PolkadotTheme.colors.fg.primary
        FundingRowIcon.IN_PROGRESS_WARNING ->
            PolkadotTheme.colors.fg.warning.copy(alpha = TINT_ALPHA) to PolkadotTheme.colors.fg.warning
        FundingRowIcon.MOVED_IN -> PolkadotTheme.colors.bg.action.primary to PolkadotTheme.colors.fg.primaryInverted
        FundingRowIcon.MOVED_OUT -> PolkadotTheme.colors.bg.surface.nested to PolkadotTheme.colors.fg.primary
        FundingRowIcon.FAILED -> PolkadotTheme.colors.fg.error.copy(alpha = TINT_ALPHA) to PolkadotTheme.colors.fg.error
    }
    val shownBackground by animateColorAsState(background, tween(PocketMotion.ICON_GLYPH_MILLIS), label = "FundingIconBackground")

    PolkadotSurface(
        modifier = Modifier.size(ICON_SIZE),
        shape = PolkadotTheme.shapes.full,
        color = if (animate) shownBackground else background,
        contentAlignment = Alignment.Center,
    ) {
        when (icon) {
            FundingRowIcon.IN_PROGRESS, FundingRowIcon.IN_PROGRESS_WARNING -> if (count != null) {
                CountRing(count = count, tint = tint)
            } else {
                Spinner(size = SPINNER_SIZE, tint = tint)
            }

            FundingRowIcon.MOVED_IN, FundingRowIcon.MOVED_OUT -> CompletedGlyph(animate = animate) {
                NovaIcon(
                    modifier = Modifier.size(GLYPH_SIZE),
                    imageVector = if (icon == FundingRowIcon.MOVED_IN) NovaIcons.CardReceived else NovaIcons.CardSent,
                    tint = tint,
                )
            }

            FundingRowIcon.FAILED -> CrossGlyph(animate = animate, tint = tint)
        }
    }
}

/** A finished glyph; on arrival the spinner it replaces fades out, then the glyph grows in. */
@Composable
private fun CompletedGlyph(animate: Boolean, glyph: @Composable () -> Unit) {
    val reduced = rememberReducedMotion()
    val reveal = remember { Animatable(if (animate && !reduced) 0f else 1f) }
    LaunchedEffect(Unit) {
        if (reveal.value < 1f) {
            reveal.animateTo(1f, tween(PocketMotion.ICON_GLYPH_MILLIS, PocketMotion.ICON_GLYPH_DELAY_MILLIS, PocketMotion.Standard))
        }
    }
    Box(contentAlignment = Alignment.Center) {
        if (reveal.value < 1f) {
            Box(
                modifier = Modifier.graphicsLayer {
                    val out = (reveal.value * SPINNER_OUT_SPEED).coerceAtMost(1f)
                    alpha = 1f - out
                    scaleX = 1f - out * SPINNER_OUT_SHRINK
                    scaleY = scaleX
                },
            ) { Spinner(size = SPINNER_SIZE, tint = PolkadotTheme.colors.fg.primary) }
        }
        Box(
            modifier = Modifier.graphicsLayer {
                alpha = reveal.value
                scaleX = GLYPH_START_SCALE + (1f - GLYPH_START_SCALE) * reveal.value
                scaleY = scaleX
            },
        ) { glyph() }
    }
}

/** A failure's cross, drawn in two strokes on arrival. */
@Composable
private fun CrossGlyph(animate: Boolean, tint: Color) {
    val reduced = rememberReducedMotion()
    val start = if (animate && !reduced) 0f else 1f
    val first = remember { Animatable(start) }
    val second = remember { Animatable(start) }
    LaunchedEffect(Unit) {
        if (first.value < 1f) {
            launch { first.animateTo(1f, tween(PocketMotion.CROSS_STROKE_MILLIS, PocketMotion.CROSS_FIRST_STROKE_DELAY_MILLIS)) }
            second.animateTo(1f, tween(PocketMotion.CROSS_STROKE_MILLIS, PocketMotion.CROSS_SECOND_STROKE_DELAY_MILLIS))
        }
    }
    Canvas(modifier = Modifier.size(CROSS_SIZE)) {
        val stroke = CROSS_STROKE.toPx()
        drawLine(tint, Offset(0f, 0f), Offset(size.width * first.value, size.height * first.value), stroke, StrokeCap.Round)
        drawLine(
            tint,
            Offset(size.width, 0f),
            Offset(size.width * (1f - second.value), size.height * second.value),
            stroke,
            StrokeCap.Round,
        )
    }
}

/**
 * How many sessions are in flight, on a turning ring. The ring grows in when it first shows, the number slides
 * with each change, and every few seconds it turns into a chevron hinting that the dock opens.
 */
@Composable
private fun CountRing(count: Int, tint: Color) {
    val reduced = rememberReducedMotion()
    val grow = remember { Animatable(if (reduced) 1f else PocketMotion.COUNT_RING_START_SCALE) }
    LaunchedEffect(Unit) {
        grow.animateTo(1f, tween(PocketMotion.COUNT_RING_MORPH_MILLIS, easing = FastOutSlowInEasing))
    }
    var hint by remember { mutableStateOf(false) }
    LaunchedEffect(count, reduced) {
        hint = false
        if (reduced) return@LaunchedEffect
        while (true) {
            delay(PocketMotion.CHEVRON_HINT_SHOWN_AT_MILLIS)
            hint = true
            delay(PocketMotion.CHEVRON_HINT_SHOWN_FOR_MILLIS)
            hint = false
            delay(PocketMotion.CHEVRON_HINT_PERIOD_MILLIS - PocketMotion.CHEVRON_HINT_SHOWN_AT_MILLIS - PocketMotion.CHEVRON_HINT_SHOWN_FOR_MILLIS)
        }
    }

    Box(contentAlignment = Alignment.Center) {
        Box(modifier = Modifier.graphicsLayer { scaleX = grow.value; scaleY = grow.value }) {
            Spinner(size = COUNT_SPINNER_SIZE, tint = tint)
        }
        AnimatedContent(
            targetState = if (hint) null else count,
            transitionSpec = {
                val up = (targetState ?: 0) >= (initialState ?: 0)
                val slide = tween<IntOffset>(PocketMotion.COUNT_SLIDE_MILLIS)
                val fade = tween<Float>(PocketMotion.COUNT_SLIDE_MILLIS)
                (slideInVertically(slide) { if (up) it / 3 else -it / 3 } + fadeIn(fade)) togetherWith
                    (slideOutVertically(slide) { if (up) -it / 3 else it / 3 } + fadeOut(fade))
            },
            label = "FundingCount",
        ) { shown ->
            if (shown == null) {
                NovaIcon(
                    modifier = Modifier
                        .size(CHEVRON_SIZE)
                        .graphicsLayer { rotationZ = CHEVRON_UP },
                    imageVector = NovaIcons.ArrowDropdown,
                    tint = tint,
                )
            } else {
                NovaText(text = shown.toString(), style = PolkadotTheme.typography.body.mediumEmphasized, color = tint)
            }
        }
    }
}

/** A row that finished while the page was open: it fades and slides into place, and unblurs where it can. */
@Composable
private fun ArrivingRow(arriving: Boolean, onArrived: () -> Unit, content: @Composable () -> Unit) {
    val reduced = rememberReducedMotion()
    val progress = remember { Animatable(if (arriving && !reduced) 0f else 1f) }
    val density = LocalDensity.current
    LaunchedEffect(Unit) {
        if (progress.value < 1f) {
            progress.animateTo(1f, tween(PocketMotion.ROW_ENTER_MILLIS, PocketMotion.ROW_ENTER_DELAY_MILLIS, PocketMotion.Standard))
        }
        if (arriving) onArrived()
    }
    val blur = PocketMotion.ROW_ENTER_BLUR * (1f - progress.value)
    Column(
        modifier = Modifier
            .graphicsLayer {
                alpha = progress.value
                translationY = with(density) { PocketMotion.ROW_ENTER_OFFSET.toPx() } * (1f - progress.value)
            }
            .then(if (blur > 0.dp) Modifier.blur(blur) else Modifier),
    ) { content() }
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
