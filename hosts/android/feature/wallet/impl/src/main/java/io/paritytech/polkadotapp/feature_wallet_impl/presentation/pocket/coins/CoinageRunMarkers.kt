package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import androidx.compose.animation.core.withInfiniteAnimationFrameNanos
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.holdings.HoldingColors
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.holdings.drawBarberPole
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.holdings.rememberHoldingColors
import kotlinx.collections.immutable.ImmutableList
import kotlin.math.roundToInt

/** One run of coins, and what to say about it. Offsets are in the strip's own coordinates. */
@Immutable
data class CoinageRun(
    val partition: CoinageStripLayout.Partition,
    val start: Float,
    val end: Float,
    val title: String,
    val amount: String
) {
    val centre: Float get() = (start + end) / 2f
}

/**
 * Rules under the summary strip saying which coins are Clearing and which are Ready.
 *
 * Replaces the composition bar. The coins are already grouped into the two partitions, so a bar above them
 * said the same thing twice; a rule under each run says it where the run is. The textures are the bar's own —
 * solid for Ready, barber pole for Clearing — so what the bar taught still reads, in the place it now applies
 * to.
 *
 * The one thing the bar did that this cannot is weight by value: it is one coin per coin, so a single large
 * coin clearing next to many small ready ones reads as a short run. The amount beside each label is there for
 * that, and says it exactly rather than proportionally. It carries the fiat symbol and not the asset's: the
 * headline directly above has already said what is being counted, and a second "CASH" on this line is clutter.
 *
 * This is a composable drawing into the window, not into the coins' own surface, which is what lets the
 * stripes keep moving while every spring is at rest. Four points of texture cost nothing worth measuring;
 * waking the coin renderer to carry them would cost a frame each time.
 */
@Composable
internal fun CoinageRunMarkers(
    modifier: Modifier = Modifier,
    runs: ImmutableList<CoinageRun>
) {
    val colors = rememberHoldingColors()
    val stripePhase = rememberStripePhase()

    Layout(
        modifier = modifier
            .fillMaxWidth()
            .height(MarkerHeight)
            // Read inside the draw block rather than in composition, so the stripes cost a redraw per frame
            // instead of a recomposition.
            .drawBehind { runs.forEach { drawRule(it, colors, stripePhase.value) } },
        content = { runs.forEach { Label(it) } }
    ) { measurables, constraints ->
        val placeables = measurables.map { it.measure(Constraints(maxWidth = constraints.maxWidth)) }
        val width = constraints.maxWidth.toFloat()
        val centres = centres(
            runs.map { it.centre * density },
            placeables.map { it.width.toFloat() },
            width,
            LabelGap.toPx()
        )

        layout(constraints.maxWidth, MarkerHeight.roundToPx()) {
            placeables.forEachIndexed { index, placeable ->
                placeable.place(
                    x = (centres[index] - placeable.width / 2f).roundToInt(),
                    y = LabelTop.roundToPx()
                )
            }
        }
    }
}

/**
 * The stripes' phase, read off the frame clock rather than driven by a repeating animation.
 *
 * A repeating animation is armed once, when its value changes, and nothing can re-arm it afterwards — so
 * anything that rebuilds the animated subtree, a relayout for instance, parks the stripes at the end of their
 * travel for good. A clock cannot get stuck, and it also puts every pole on screen in step.
 */
@Composable
private fun rememberStripePhase(): State<Float> {
    val phase = remember { mutableFloatStateOf(0f) }

    LaunchedEffect(Unit) {
        while (true) {
            withInfiniteAnimationFrameNanos { nanos ->
                phase.floatValue = (nanos % STRIPE_PERIOD_NANOS).toFloat() / STRIPE_PERIOD_NANOS
            }
        }
    }

    return phase
}

@Composable
private fun Label(run: CoinageRun) {
    Row(horizontalArrangement = Arrangement.spacedBy(LabelSpacing)) {
        NovaText(
            text = run.title,
            maxLines = 1,
            style = PolkadotTheme.typography.body.small,
            color = PolkadotTheme.colors.fg.secondary
        )

        NovaText(
            text = run.amount,
            maxLines = 1,
            style = PolkadotTheme.typography.body.small,
            color = PolkadotTheme.colors.fg.primary
        )
    }
}

/**
 * One run's rule: a capsule of texture, then the outline that keeps it visible.
 *
 * Both textures are mostly white and the card behind them is not always dark, so without the outline a rule
 * disappears on a light theme. It is inset rather than straddling the edge, so it does not eat the pattern it
 * is there to make visible.
 *
 * The stripes are drawn with a repeating gradient, which is defined over the whole plane, so neither of the
 * two things a tiled layer needs — a pattern drawn past its own bounds, and a layer wide enough to survive a
 * period of travel — applies here. The seam is still exact: the origin slides by one whole period along the
 * gradient's own axis per cycle, so the pattern at phase 1 is the pattern at phase 0.
 */
private fun DrawScope.drawRule(run: CoinageRun, colors: HoldingColors, stripePhase: Float) {
    val height = RuleHeight.toPx()
    val radius = height / 2f
    val left = run.start * density
    val width = maxOf((run.end - run.start) * density, height)

    when (run.partition) {
        CoinageStripLayout.Partition.READY -> drawRoundRect(
            color = colors.spendable,
            topLeft = Offset(left, 0f),
            size = Size(width, height),
            cornerRadius = CornerRadius(radius, radius)
        )

        CoinageStripLayout.Partition.CLEARING -> drawBarberPole(
            left = left,
            top = 0f,
            width = width,
            height = height,
            cornerRadius = radius,
            stripePeriod = StripePeriod.toPx(),
            phase = stripePhase,
            colors = colors
        )
    }

    val stroke = OutlineWidth.toPx()
    val inset = stroke / 2f

    drawRoundRect(
        color = colors.frame,
        topLeft = Offset(left + inset, inset),
        size = Size(width - stroke, height - stroke),
        cornerRadius = CornerRadius(radius - inset, radius - inset),
        style = Stroke(width = stroke)
    )
}

/**
 * Where each label sits: under the middle of its run, but never past the margins and never on top of its
 * neighbour.
 *
 * A run of one coin at the very edge would otherwise hang its label over the side, and two short runs at
 * opposite ends both pull toward the middle. Forwards to separate them, then backwards from the right edge,
 * which is the usual way to settle a row of labels that each want a place and together may not fit.
 */
internal fun centres(
    wanted: List<Float>,
    widths: List<Float>,
    width: Float,
    gap: Float
): List<Float> {
    if (wanted.isEmpty() || width <= 0f) return wanted

    val centres = wanted.mapIndexed { index, centre ->
        val label = widths[index]

        centre.coerceIn(label / 2f, maxOf(width - label / 2f, label / 2f))
    }.toMutableList()

    for (index in 1 until centres.size) {
        val room = (widths[index - 1] + widths[index]) / 2f + gap

        centres[index] = maxOf(centres[index], centres[index - 1] + room)
    }

    for (index in centres.indices.reversed()) {
        centres[index] = minOf(centres[index], width - widths[index] / 2f)

        if (index > 0) {
            val room = (widths[index - 1] + widths[index]) / 2f + gap

            centres[index - 1] = minOf(centres[index - 1], centres[index] - room)
        }
    }

    return centres.mapIndexed { index, centre -> maxOf(centre, widths[index] / 2f) }
}

/** Total height of a marker: the rule, the gap under it, then the label. */
internal val MarkerHeight = 30.dp

/**
 * Four points, with capsule ends. Below this anything short of a capsule is indistinguishable from one, and
 * the fully rounded ends read as a deliberate stop; the fourth point pays for the outline, so the amount of
 * visible texture is what three points of bare rule used to be.
 */
private val RuleHeight = 4.dp

private val OutlineWidth = 0.5.dp
private val LabelTop = 11.dp
private val LabelSpacing = 4.dp

/** Between two labels that have both been pushed toward the middle. */
private val LabelGap = 12.dp

/** One red stripe and one white one: five points each, measured across the stripes. */
private val StripePeriod = 10.dp

/** One period of travel. Slow enough to read as a turning pole rather than as something hurrying. */
private const val STRIPE_PERIOD_NANOS = 600_000_000L
