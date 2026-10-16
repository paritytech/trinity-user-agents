package io.paritytech.polkadotapp.feature_connection_status_api.presentation

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.AssetHub
import io.paritytech.polkadotapp.design.components.icon.vectors.Bulletin
import io.paritytech.polkadotapp.design.components.icon.vectors.People
import io.paritytech.polkadotapp.design.components.icon.vectors.StatementStore
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.tooltip.NonFocusablePopupProperties
import io.paritytech.polkadotapp.design.components.tooltip.PolkadotTooltip
import io.paritytech.polkadotapp.design.components.tooltip.PolkadotTooltipContent
import io.paritytech.polkadotapp.design.components.tooltip.TooltipAlignment
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicator
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicatorsModel
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthItemModel
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.IndicatorRow
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ProductionBand
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin
import kotlin.time.Duration.Companion.seconds
import io.paritytech.polkadotapp.common.R as RCommon

private const val PULSE_MIN_ALPHA = 0.3f
private const val PULSE_DURATION_MS = 900
private const val TOP_ANGLE = -90f
private const val FULL_SWEEP = 360f
private const val ARC_ANIMATION_MS = 600
private const val SCALLOP_LOBES = 11
private const val SCALLOP_TROUGH_RATIO = 0.965f
private const val STEPS_PER_LOBE = 12
private const val TWO_PI = 2 * PI.toFloat()
private const val DOT_COUNT = 16

private const val PREVIEW_EXPECTED_BLOCKS = 15
private const val PREVIEW_BLOCKS_PER_ROW = 8
private val PREVIEW_BLOCK_TIME = 2.seconds

// The design leaves the ring open exactly where the cross sits rather than reporting how far
// production has fallen. Round caps eat into the gap, so 90 degrees of path reads as 77 on screen.
private const val NOT_PRODUCING_GAP = 90f

// 225 degrees: the upper left, where the design leaves the ring open for the cross. Both cross ratios
// are measured off the mock-up against the ring's outer diameter; deriving the arm from the whole
// extent keeps the drawn size right at either indicator size, whose strokes are not in proportion.
private const val CROSS_ANGLE = 225f
private val CROSS_ANGLE_RADIANS = CROSS_ANGLE * PI.toFloat() / 180f
private const val CROSS_EXTENT_RATIO = 0.308f
private const val CROSS_RADIUS_RATIO = 0.483f

/**
 * The three lengths one indicator is drawn from. [Bar] is the top-bar and tab-bar size; [Panel] is the
 * same drawing at the size the "Network Status" rows use.
 */
@Immutable
data class ChainIndicatorSize(
    val diameter: Dp,
    val glyph: Dp,
    val ringStroke: Dp,
) {
    companion object {
        val Bar = ChainIndicatorSize(diameter = 20.dp, glyph = 10.dp, ringStroke = 2.dp)
        val Panel = ChainIndicatorSize(diameter = 36.dp, glyph = 18.dp, ringStroke = 3.dp)
    }
}

object ChainHealthBarDefaults {
    /**
     * Height of the bar's content row, excluding the status-bar inset. The root also inflates the
     * content's top window inset by this amount so screens sit below the bar while their backgrounds
     * still draw full-bleed behind it.
     */
    val ContentHeight = ChainIndicatorSize.Bar.diameter
}

/** Always-on, transparent, overlaid at the very top like the system status indicators. */
@Composable
fun ChainHealthBar(
    model: ChainHealthIndicatorsModel,
    tooltipVisible: Boolean,
    onTooltipDismiss: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .statusBarsPadding()
            .height(ChainHealthBarDefaults.ContentHeight)
            .padding(horizontal = PolkadotTheme.spacings.medium),
        horizontalArrangement = Arrangement.End,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box {
            ChainHealthIndicators(model = model)

            PolkadotTooltip(
                expanded = tooltipVisible,
                onDismiss = onTooltipDismiss,
                arrowVisible = true,
                alignment = TooltipAlignment.Bottom,
                shape = PolkadotTheme.shapes.tiny,
                properties = NonFocusablePopupProperties,
            ) {
                PolkadotTooltipContent(
                    title = stringResource(RCommon.string.chain_health_tooltip_title),
                    message = stringResource(RCommon.string.chain_health_tooltip_message),
                    onDismiss = onTooltipDismiss,
                )
            }
        }
    }
}

/**
 * One indicator per monitored chain, plus one for the statement store. The inner glyph names the row; the
 * disc or ring around it draws [ChainHealthIndicator]. Display-only — the row carries no press target of
 * its own.
 */
@Composable
fun ChainHealthIndicators(
    modifier: Modifier = Modifier,
    model: ChainHealthIndicatorsModel,
    indicatorSize: ChainIndicatorSize = ChainIndicatorSize.Bar,
) {
    CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides Dp.Unspecified) {
        Row(
            modifier = modifier,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            model.rows.forEach { item -> RowIndicator(item = item, indicatorSize = indicatorSize) }
        }
    }
}

@Composable
internal fun RowIndicator(
    modifier: Modifier = Modifier,
    item: ChainHealthItemModel,
    indicatorSize: ChainIndicatorSize,
) {
    val description = stringResource(
        RCommon.string.chain_health_indicator_description,
        stringResource(item.row.nameRes()),
        stringResource(item.labelRes()),
    )

    Box(modifier = modifier) {
        Box(
            modifier = Modifier
                .size(indicatorSize.diameter)
                .semantics { contentDescription = description },
            contentAlignment = Alignment.Center,
        ) {
            when (val indicator = item.indicator) {
                is ChainHealthIndicator.Healthy -> HealthyDisc(item, indicatorSize)
                ChainHealthIndicator.Outage -> NotProducingRing(item, indicatorSize)
                is ChainHealthIndicator.Production -> ProductionArc(item, indicator, indicatorSize)
                ChainHealthIndicator.Connecting -> ConnectingRing(item, indicatorSize)
                ChainHealthIndicator.Disconnected, ChainHealthIndicator.Offline -> DottedRing(item, indicatorSize)
            }
        }
    }
}

@Composable
private fun HealthyDisc(item: ChainHealthItemModel, indicatorSize: ChainIndicatorSize) {
    PolkadotSurface(
        modifier = Modifier.fillMaxSize(),
        shape = CircleShape,
        color = PolkadotTheme.colors.fg.primary,
        contentAlignment = Alignment.Center,
    ) {
        Glyph(item = item, tint = PolkadotTheme.colors.fg.primaryInverted, indicatorSize = indicatorSize)
    }
}

@Composable
private fun NotProducingRing(item: ChainHealthItemModel, indicatorSize: ChainIndicatorSize) {
    val crossColor = PolkadotTheme.colors.fg.disabled
    ArcRing(
        startAngle = CROSS_ANGLE + NOT_PRODUCING_GAP / 2,
        sweepAngle = FULL_SWEEP - NOT_PRODUCING_GAP,
        color = PolkadotTheme.colors.stroke.secondary,
        indicatorSize = indicatorSize,
    )
    Glyph(item = item, tint = PolkadotTheme.colors.fg.disabled, indicatorSize = indicatorSize)
    // Painted last so it sits above the glyph.
    Box(
        modifier = Modifier
            .fillMaxSize()
            .drawWithCache {
                val stroke = indicatorSize.ringStroke.toPx()
                val centre = Offset(size.width / 2, size.height / 2)
                val crossRadius = size.minDimension * CROSS_RADIUS_RATIO
                val crossCentre = centre + Offset(
                    crossRadius * cos(CROSS_ANGLE_RADIANS),
                    crossRadius * sin(CROSS_ANGLE_RADIANS),
                )
                val arm = (size.minDimension * CROSS_EXTENT_RATIO - stroke) / 2
                onDrawBehind {
                    drawLine(crossColor, crossCentre - Offset(arm, arm), crossCentre + Offset(arm, arm), stroke, StrokeCap.Round)
                    drawLine(crossColor, crossCentre + Offset(arm, -arm), crossCentre - Offset(arm, -arm), stroke, StrokeCap.Round)
                }
            },
    )
}

@Composable
private fun ProductionArc(
    item: ChainHealthItemModel,
    indicator: ChainHealthIndicator.Production,
    indicatorSize: ChainIndicatorSize,
) {
    val color = when (indicator.band) {
        ProductionBand.Plain -> PolkadotTheme.colors.fg.primary
        ProductionBand.Warning -> PolkadotTheme.colors.fg.warning
        ProductionBand.Error -> PolkadotTheme.colors.fg.error
    }
    ArcRing(
        // The design fills the ring backwards from the top, so the sweep is negative.
        startAngle = TOP_ANGLE,
        sweepAngle = -indicator.liveness.share * FULL_SWEEP,
        color = color,
        indicatorSize = indicatorSize,
        trackColor = PolkadotTheme.colors.stroke.secondary,
    )
    Glyph(item = item, tint = PolkadotTheme.colors.fg.primary, indicatorSize = indicatorSize)
}

@Composable
private fun ArcRing(
    startAngle: Float,
    sweepAngle: Float,
    color: Color,
    indicatorSize: ChainIndicatorSize,
    trackColor: Color? = null,
) {
    // Read inside onDrawBehind so a moving arc invalidates the draw only, not the cached geometry.
    val animatedSweep = animateFloatAsState(
        targetValue = sweepAngle,
        animationSpec = tween(ARC_ANIMATION_MS),
        label = "ChainIndicatorArc",
    )
    Box(
        modifier = Modifier
            .fillMaxSize()
            .drawWithCache {
                val stroke = indicatorSize.ringStroke.toPx()
                val trackStyle = Stroke(stroke)
                val arcStyle = Stroke(width = stroke, cap = StrokeCap.Round)
                val radius = (size.minDimension - stroke) / 2
                val inset = Offset(stroke / 2, stroke / 2)
                val arcSize = Size(size.width - stroke, size.height - stroke)
                onDrawBehind {
                    if (trackColor != null) {
                        drawCircle(color = trackColor, radius = radius, style = trackStyle)
                    }
                    drawArc(
                        color = color,
                        startAngle = startAngle,
                        sweepAngle = animatedSweep.value,
                        useCenter = false,
                        topLeft = inset,
                        size = arcSize,
                        style = arcStyle,
                    )
                }
            },
    )
}

@Composable
private fun ConnectingRing(item: ChainHealthItemModel, indicatorSize: ChainIndicatorSize) {
    ScallopRing(color = PolkadotTheme.colors.stroke.secondary, indicatorSize = indicatorSize)
    val pulse = rememberInfiniteTransition(label = "ChainConnectingPulse")
    val alpha by pulse.animateFloat(
        initialValue = 1f,
        targetValue = PULSE_MIN_ALPHA,
        animationSpec = infiniteRepeatable(tween(PULSE_DURATION_MS), RepeatMode.Reverse),
        label = "ChainConnectingGlyphAlpha",
    )
    Box(modifier = Modifier.graphicsLayer { this.alpha = alpha }) {
        Glyph(item = item, tint = PolkadotTheme.colors.fg.primary, indicatorSize = indicatorSize)
    }
}

@Composable
private fun DottedRing(item: ChainHealthItemModel, indicatorSize: ChainIndicatorSize) {
    val ringColor = PolkadotTheme.colors.stroke.secondary
    Box(
        modifier = Modifier
            .fillMaxSize()
            .drawWithCache {
                val stroke = indicatorSize.ringStroke.toPx()
                val radius = (size.minDimension - stroke) / 2
                // Deriving the period from the circumference keeps the dots from bunching at the seam, and
                // the on-length is zero because a round cap already draws a dot one stroke wide at each end.
                val period = TWO_PI * radius / DOT_COUNT
                val style = Stroke(
                    width = stroke,
                    cap = StrokeCap.Round,
                    pathEffect = PathEffect.dashPathEffect(floatArrayOf(0f, period)),
                )
                onDrawBehind { drawCircle(color = ringColor, radius = radius, style = style) }
            },
    )
    Glyph(item = item, tint = PolkadotTheme.colors.fg.disabled, indicatorSize = indicatorSize)
}

@Composable
private fun ScallopRing(color: Color, indicatorSize: ChainIndicatorSize) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .drawWithCache {
                val stroke = indicatorSize.ringStroke.toPx()
                val crest = (size.minDimension - stroke) / 2
                val centre = Offset(size.width / 2, size.height / 2)
                val path = scallopPath(centre, crest, crest * SCALLOP_TROUGH_RATIO)
                val style = Stroke(width = stroke)
                onDrawBehind { drawPath(path = path, color = color, style = style) }
            },
    )
}

// Sampled as a polyline because a cosine-modulated radius has no path-verb equivalent.
private fun scallopPath(centre: Offset, crest: Float, trough: Float): Path {
    val path = Path()
    val steps = SCALLOP_LOBES * STEPS_PER_LOBE
    for (step in 0..steps) {
        val angle = TWO_PI * step / steps
        val radius = trough + (crest - trough) * (1 + cos(SCALLOP_LOBES * angle)) / 2
        val x = centre.x + radius * cos(angle)
        val y = centre.y + radius * sin(angle)
        if (step == 0) path.moveTo(x, y) else path.lineTo(x, y)
    }
    path.close()
    return path
}

@Composable
private fun Glyph(item: ChainHealthItemModel, tint: Color, indicatorSize: ChainIndicatorSize) {
    NovaIcon(
        modifier = Modifier.requiredSize(indicatorSize.glyph),
        imageVector = item.row.imageVector(),
        tint = tint,
        contentDescription = null,
    )
}

private fun IndicatorRow.imageVector(): ImageVector = when (this) {
    IndicatorRow.People -> NovaIcons.People
    IndicatorRow.AssetHub -> NovaIcons.AssetHub
    IndicatorRow.Bulletin -> NovaIcons.Bulletin
    IndicatorRow.StatementStore -> NovaIcons.StatementStore
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChainHealthIndicatorsPreview() {
    PolkadotTheme {
        ChainHealthIndicators(
            modifier = Modifier.padding(PolkadotTheme.spacings.medium),
            model = ChainHealthIndicatorsModel(
                persistentListOf(
                    previewItem(IndicatorRow.People, ChainHealthIndicator.of(share = 1f, PREVIEW_BLOCK_TIME)),
                    previewItem(IndicatorRow.AssetHub, ChainHealthIndicator.Outage),
                    previewItem(IndicatorRow.Bulletin, ChainHealthIndicator.of(share = 0.8f, PREVIEW_BLOCK_TIME)),
                    previewItem(IndicatorRow.People, ChainHealthIndicator.of(share = 0.4f, PREVIEW_BLOCK_TIME)),
                    previewItem(IndicatorRow.AssetHub, ChainHealthIndicator.of(share = 0.2f, PREVIEW_BLOCK_TIME)),
                    previewItem(IndicatorRow.AssetHub, ChainHealthIndicator.Connecting),
                    previewItem(IndicatorRow.Bulletin, ChainHealthIndicator.Disconnected),
                    previewItem(IndicatorRow.People, ChainHealthIndicator.Offline),
                    previewItem(IndicatorRow.StatementStore, ChainHealthIndicator.Healthy(liveness = null)),
                    previewItem(IndicatorRow.StatementStore, ChainHealthIndicator.Connecting),
                    previewItem(IndicatorRow.StatementStore, ChainHealthIndicator.Disconnected),
                ),
            ),
        )
    }
}

// No state of the app steps the arc through its range, so the gradation is only visible here.
@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChainProductionScalePreview() {
    PolkadotTheme {
        Column(
            modifier = Modifier.padding(PolkadotTheme.spacings.medium),
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
        ) {
            for (produced in (0..PREVIEW_EXPECTED_BLOCKS).chunked(PREVIEW_BLOCKS_PER_ROW)) {
                ChainHealthIndicators(
                    model = ChainHealthIndicatorsModel(
                        produced
                            .map { blocks ->
                                val share = blocks.toFloat() / PREVIEW_EXPECTED_BLOCKS
                                previewItem(IndicatorRow.People, ChainHealthIndicator.of(share, PREVIEW_BLOCK_TIME))
                            }
                            .toImmutableList(),
                    ),
                )
            }
        }
    }
}

private fun previewItem(row: IndicatorRow, indicator: ChainHealthIndicator) = ChainHealthItemModel(
    row = row,
    indicator = indicator,
)
