package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.utils.rememberReducedMotion
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenSymbolAppearance
import java.math.BigDecimal
import java.math.RoundingMode

/** The Pocket page's timings, from the Variant 2 prototype. */
object PocketMotion {
    val COLLAPSE_RANGE = 150.dp
    const val COLLAPSED_CARD_WIDTH = 60f
    const val CARD_WIDTH = 366f
    const val TITLE_FADE_RATE = 2.5f
    const val BALANCE_FADE_RATE = 1.8f

    const val BALANCE_COUNT_UP_MILLIS = 650
    const val STRIPE_PERIOD_MILLIS = 700
    val EaseOutCubic = CubicBezierEasing(0.33f, 1f, 0.68f, 1f)
}

/** How far the card has collapsed into the header, and where it sits on the way. */
object PocketCollapse {
    /** Eased progress for [scrolledPx] of a [rangePx] range: 0 at rest, 1 docked, smoothstep in between. */
    fun progress(scrolledPx: Float, rangePx: Float): Float {
        if (rangePx <= 0f) return 1f
        return smoothstep((scrolledPx / rangePx).coerceIn(0f, 1f))
    }

    fun smoothstep(p: Float): Float = p * p * (3f - 2f * p)

    fun scale(progress: Float): Float = lerp(1f, PocketMotion.COLLAPSED_CARD_WIDTH / PocketMotion.CARD_WIDTH, progress)

    fun titleAlpha(progress: Float): Float = (1f - PocketMotion.TITLE_FADE_RATE * progress).coerceIn(0f, 1f)

    fun balanceAlpha(progress: Float): Float = (1f - PocketMotion.BALANCE_FADE_RATE * progress).coerceIn(0f, 1f)

    /**
     * The card's top-left at [progress], scaled from its top-left: from where the list puts it to centred on
     * ([dockCenterX], [dockCenterY]).
     */
    fun offset(
        progress: Float,
        restX: Float,
        restY: Float,
        dockCenterX: Float,
        dockCenterY: Float,
        width: Float,
        height: Float,
    ): Pair<Float, Float> {
        val scale = scale(progress)
        val dockX = dockCenterX - width * scale / 2f
        val dockY = dockCenterY - height * scale / 2f
        return lerp(restX, dockX, progress) to lerp(restY, dockY, progress)
    }

    private fun lerp(from: Float, to: Float, t: Float) = from + (to - from) * t
}

/** The figure a count-up shows at [fraction] of the way from [from] to [to], to the cent. */
fun countUpValue(from: BigDecimal, to: BigDecimal, fraction: Float): BigDecimal {
    val step = (to - from).multiply(fraction.coerceIn(0f, 1f).toBigDecimal())
    return (from + step).setScale(to.scale().coerceAtLeast(2), RoundingMode.DOWN)
}

/**
 * [amount], counting up over [PocketMotion.BALANCE_COUNT_UP_MILLIS] when it grows, from the figure on screen: the
 * last total, or where the count stood when a newer total arrives part-way through it.
 */
@Composable
fun rememberCountUp(amount: TokenAmountModel): TokenAmountModel {
    val reduced = rememberReducedMotion()
    val shown = remember { ShownFigure(amount.amount) }
    // Keyed on the total, so a grown one starts at the shown figure in the composition it arrives in, never drawn
    // for a frame before its count starts.
    val (from, fraction) = remember(amount.amount) {
        val from = shown.value
        from to Animatable(if (!reduced && amount.amount > from) 0f else 1f)
    }

    LaunchedEffect(fraction) {
        fraction.animateTo(1f, tween(PocketMotion.BALANCE_COUNT_UP_MILLIS, easing = PocketMotion.EaseOutCubic))
    }

    val progress = fraction.value
    val value = if (progress >= 1f) amount.amount else countUpValue(from, amount.amount, progress)
    SideEffect { shown.value = value }
    return if (progress >= 1f) amount else CountedAmount(value, amount.appearance)
}

/** The figure last drawn; not state, as nothing is drawn from it. */
private class ShownFigure(var value: BigDecimal)

private class CountedAmount(
    override val amount: BigDecimal,
    override val appearance: TokenSymbolAppearance,
) : TokenAmountModel
