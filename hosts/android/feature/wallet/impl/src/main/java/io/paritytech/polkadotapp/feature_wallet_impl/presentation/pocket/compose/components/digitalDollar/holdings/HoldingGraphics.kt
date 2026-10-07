package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.holdings

import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TileMode
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

/**
 * The sizes the coinage card is laid out from.
 *
 * Plain `Dp` constants rather than theme spacings, which are for paddings and margins: each value here is an
 * element size or a corner radius.
 */
internal object HoldingGeometry {
    /** Summary block. */
    val containerCorner = 24.dp
    val containerPadding = 16.dp
    val containerSpacing = 12.dp
    val headlineSpacing = 4.dp
}

/**
 * The colours the holdings marks are drawn in.
 *
 * The division is the load-bearing rule. A *pinned* colour carries status and is the same on every theme:
 * `fg.error` is #D9363E and `bg.status.warning` is #F59E0B on all five, so "spendable" cannot change colour
 * underneath the reader. Reaching for a theme-following token here would destroy the meaning — `fg.primary`
 * runs from #F4F4F5 on BerlinNight to #431407 on Lisbon.
 *
 * `bg.status.warning` rather than `fg.warning` for the same reason: `fg.warning` splits, #F59E0B dark against
 * #D17F00 light.
 */
@Immutable
internal data class HoldingColors(
    /** Spendable, unloadable, inner dots, and the ground the stripes sit on. */
    val spendable: Color,
    /** Not spendable, the hop disc, the stripes, and the upper unknown bar. */
    val notSpendable: Color,
    val unknownLower: Color,
    val chipOutline: Color,
    val chipLabel: Color,
) {
    /**
     * A literal, not a token: it has to stay dark on light surfaces and is what makes an unframed white or
     * orange mark legible at all — orange measures under 2:1 against a light surface on its own. It measures
     * 18:1 against white and 15:1 or better against every light surface here; on BerlinNight it merges into
     * the background, which costs nothing because white already measures 19:1 there.
     */
    val frame: Color = MARK_FRAME
}

@Composable
internal fun rememberHoldingColors(): HoldingColors {
    val spendable = PolkadotTheme.colors.fg.staticWhite
    val notSpendable = PolkadotTheme.colors.fg.error
    val unknownLower = PolkadotTheme.colors.bg.status.warning
    val chipOutline = PolkadotTheme.colors.stroke.tertiary
    val chipLabel = PolkadotTheme.colors.fg.secondary

    return remember(spendable, notSpendable, unknownLower, chipOutline, chipLabel) {
        HoldingColors(
            spendable = spendable,
            notSpendable = notSpendable,
            unknownLower = unknownLower,
            chipOutline = chipOutline,
            chipLabel = chipLabel,
        )
    }
}

/**
 * Diagonal stripes sliding rightwards, drawn as one repeating gradient rather than a stack of paths.
 *
 * Rightwards is the way a barber's pole turns given stripes that lean right, which these do.
 *
 * The gradient axis runs perpendicular to the stripes, so a lean of [STRIPE_LEAN] horizontal per unit of
 * height puts the stripes along `(lean, 1)` and the axis along `(1, -lean)`. Its length is one whole period,
 * which is what lets the wrap be seamless: the origin slides *along that axis* by exactly one period per
 * cycle, so the pattern at phase 1 is the pattern at phase 0. Sliding it horizontally instead would project
 * onto the axis as a fraction of a period and jump the stripes on every wrap.
 *
 * The two stops meeting at [STRIPE_SPLIT] are a hair apart so the edge stays crisp without relying on the
 * renderer accepting two stops at one position.
 */
internal fun DrawScope.drawBarberPole(
    left: Float,
    top: Float,
    width: Float,
    height: Float,
    cornerRadius: Float,
    stripePeriod: Float,
    phase: Float,
    colors: HoldingColors,
) {
    if (width <= 0f) return

    val axis = barberPoleAxis(stripePeriod)
    val origin = barberPoleOrigin(left, top, stripePeriod, phase)

    drawRoundRect(
        brush = Brush.linearGradient(
            colorStops = arrayOf(
                0f to colors.notSpendable,
                STRIPE_SPLIT to colors.notSpendable,
                STRIPE_SPLIT + STRIPE_EDGE to colors.spendable,
                1f to colors.spendable
            ),
            start = origin,
            end = origin + axis,
            tileMode = TileMode.Repeated
        ),
        topLeft = Offset(left, top),
        size = Size(width, height),
        cornerRadius = CornerRadius(cornerRadius, cornerRadius)
    )
}

/**
 * The gradient's axis: perpendicular to the stripes, one whole period long.
 *
 * Leans right and up, which puts the stripes themselves along down-right — the lean a barber's pole has.
 * Pulled out of the drawing so the travel direction can be pinned by a test rather than by an eye on an
 * animation that is over in six hundred milliseconds.
 */
internal fun barberPoleAxis(stripePeriod: Float): Offset {
    val direction = Offset(1f, -STRIPE_LEAN)

    return direction / direction.getDistance() * stripePeriod
}

/**
 * Where the repeating gradient starts at a given phase.
 *
 * Sliding the origin *along the axis* is what makes the wrap seamless: one period of phase moves it by
 * exactly one period, so the pattern at phase 1 is the pattern at phase 0. Sliding it horizontally instead
 * would project onto the axis as a fraction of a period and jump the stripes on every wrap.
 *
 * The origin advances with the phase, and so does the point where any given colour sits, which is what makes
 * the stripes travel rightwards.
 */
internal fun barberPoleOrigin(left: Float, top: Float, stripePeriod: Float, phase: Float): Offset =
    Offset(left, top) + barberPoleAxis(stripePeriod) * phase

private val MARK_FRAME = Color(0xFF141418)

private const val STRIPE_SPLIT = 0.5f
private const val STRIPE_EDGE = 0.001f

/** Horizontal travel per unit of height, which is what sets the stripes' slant. */
private const val STRIPE_LEAN = 0.7f
