package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import android.animation.ValueAnimator
import android.graphics.BlurMaskFilter
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.Easing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameMillis
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsPanFeel
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import kotlin.math.max
import kotlin.math.pow
import kotlin.math.sqrt

// The modifiers that style a node by value (renderer modifiers 12 to 19): each draws what the CSS it copies draws.

/** The arithmetic of the styled modifiers and a gesture's feel, kept apart from drawing so it can be tested. */
object WidgetStyleRules {
    /** 0xRRGGBBAA as a colour. */
    fun color(rgba: Long): Color = Color(
        red = ((rgba shr 24) and 0xFF).toInt(),
        green = ((rgba shr 16) and 0xFF).toInt(),
        blue = ((rgba shr 8) and 0xFF).toInt(),
        alpha = (rgba and 0xFF).toInt(),
    )

    /** `1 - rate × progress`, both clamped. */
    fun panFade(progress: Float, rate: Float): Float = (1f - rate * progress.coerceIn(0f, 1f)).coerceIn(0f, 1f)

    /** The band's centre from the node's start (dp), or null while it rests. */
    fun shimmerCenter(shimmer: JsModifier.Shimmer, widthDp: Float, elapsedMs: Long): Float? {
        if (shimmer.speed <= 0f) return null
        val sweepMs = (widthDp + 2 * shimmer.lead) / shimmer.speed * MS_PER_S
        val period = sweepMs + shimmer.restMs
        val time = (elapsedMs % period.toDouble()).toFloat()
        return if (time < sweepMs) -shimmer.lead + shimmer.speed * time / MS_PER_S else null
    }

    /** CSS's blur length is twice the Gaussian's deviation; Skia's mask radius maps to a deviation as 0.57735 r + 0.5. */
    fun blurMaskRadius(blurPx: Float): Float = ((blurPx / 2f - SKIA_SIGMA_OFFSET) / SKIA_RADIUS_TO_SIGMA).coerceAtLeast(0f)

    /** How long the way to the end takes: the approach, longer over a longer way. */
    fun approachMs(feel: JsPanFeel, distanceDp: Float): Int =
        (feel.approachMs * max(1f, sqrt(max(distanceDp, 1f) / feel.approachReference))).toInt()

    /** The pop at [frames] sixtieths after it starts, from [start]: the scale and dp past the end; null once over. */
    fun pop(feel: JsPanFeel, frames: Float, start: Float): Pair<Float, Float>? {
        val table = feel.pop
        if (table.size < 2 || frames >= table.size - 1) return null
        val scale = if (frames <= 0f) {
            start
        } else {
            val index = frames.toInt()
            val fraction = frames - index
            val from = if (index == 0) start else table[index]
            from + (table[index + 1] - from) * fraction
        }
        val peak = table.max()
        val past = if (scale > 1f && peak > 1f) feel.popOffset * ((scale - 1f) / (peak - 1f)).coerceIn(0f, 1f).pow(1.5f) else 0f
        return scale to past
    }

    /** A quadratic ease-out, `1 - (1 - u)²`, which is exactly this cubic Bézier. */
    val EaseOutQuad: Easing = CubicBezierEasing(1f / 3f, 2f / 3f, 2f / 3f, 1f)

    /** CSS's `ease`. */
    val CssEase: Easing = CubicBezierEasing(0.25f, 0.1f, 0.25f, 1f)

    private const val MS_PER_S = 1000f
    private const val SKIA_RADIUS_TO_SIGMA = 0.57735f
    private const val SKIA_SIGMA_OFFSET = 0.5f
}

// MARK: Places of keyed gestures

/** Where a keyed gesture's children stand along their travel, 0 to 1, for the nodes that fade with it. */
class GesturePanPlace {
    var progress: () -> Float by mutableStateOf({ 0f })
}

class GesturePanPlaces {
    private val places = mutableMapOf<String, GesturePanPlace>()

    fun place(key: String): GesturePanPlace = places.getOrPut(key) { GesturePanPlace() }
}

val LocalGesturePanPlaces = staticCompositionLocalOf { GesturePanPlaces() }

// MARK: Modifiers

/** Milliseconds since the caller first drew, a frame at a time; read in a draw or layer block, it redraws only. */
@Composable
internal fun rememberFrameMillis(): State<Long> = produceState(0L) {
    val start = withFrameMillis { it }
    while (true) withFrameMillis { value = it - start }
}

internal fun JsModifier.Fill.toFillModifier(shape: Shape): Modifier = Modifier.background(WidgetStyleRules.color(rgba), shape)

/** CSS `box-shadow` under the node's fill shape: moved, blurred, taking no room and clipped by nothing. */
internal fun JsModifier.Shadow.toShadowModifier(shape: Shape): Modifier = Modifier.drawBehind {
    val outline = shape.createOutline(size, layoutDirection, this)
    val paint = Paint().apply { color = WidgetStyleRules.color(rgba) }
    val radius = WidgetStyleRules.blurMaskRadius(blur.dp.toPx())
    if (radius > 0f) paint.asFrameworkPaint().maskFilter = BlurMaskFilter(radius, BlurMaskFilter.Blur.NORMAL)
    drawIntoCanvas { canvas ->
        canvas.save()
        canvas.translate(x.dp.toPx(), y.dp.toPx())
        canvas.drawOutline(outline, paint)
        canvas.restore()
    }
}

internal fun JsModifier.Translate.toTranslateModifier(): Modifier = Modifier.offset(x.dp, y.dp)

@Composable
internal fun JsModifier.FadeWithPan.toFadeModifier(): Modifier {
    val place = LocalGesturePanPlaces.current.place(key)
    return Modifier.graphicsLayer { alpha = WidgetStyleRules.panFade(place.progress(), rate) }
}

@Composable
internal fun JsModifier.Spin.toSpinModifier(): Modifier {
    if (periodMs <= 0 || !ValueAnimator.areAnimatorsEnabled()) return Modifier
    val time = rememberFrameMillis()
    return Modifier.graphicsLayer { rotationZ = (time.value % periodMs) / periodMs.toFloat() * FULL_TURN }
}

/** A band of light across the node's drawn pixels only: drawn over them in their own layer, atop what they cover. */
@Composable
internal fun JsModifier.Shimmer.toShimmerModifier(): Modifier {
    if (!ValueAnimator.areAnimatorsEnabled()) return Modifier
    val time = rememberFrameMillis()
    val shimmer = this
    val color = WidgetStyleRules.color(rgba)
    val last = (stops.size - 1).coerceAtLeast(1)
    val colorStops = stops.mapIndexed { index, alpha -> index / last.toFloat() to color.copy(alpha = color.alpha * alpha) }
        .toTypedArray()
    return Modifier
        .graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
        .drawWithContent {
            drawContent()
            val center = WidgetStyleRules.shimmerCenter(shimmer, size.width / density, time.value) ?: return@drawWithContent
            val bandPx = band.dp.toPx()
            val start = center.dp.toPx() - bandPx / 2
            drawRect(
                brush = Brush.horizontalGradient(*colorStops, startX = start, endX = start + bandPx),
                blendMode = BlendMode.SrcAtop,
            )
        }
}

/** A text's type by value on the design system's Inter: a CSS line box, the glyphs centred in it, no font padding. */
@Composable
internal fun JsModifier.Font.toTextStyle(): TextStyle {
    val density = LocalDensity.current
    return with(density) {
        PolkadotTheme.typography.body.large.copy(
            fontSize = size.dp.toSp(),
            lineHeight = lineHeight.dp.toSp(),
            fontWeight = FontWeight(weight.coerceIn(1, MAX_WEIGHT)),
            letterSpacing = letterSpacing.dp.toSp(),
            color = WidgetStyleRules.color(rgba),
            lineHeightStyle = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None),
            platformStyle = PlatformTextStyle(includeFontPadding = false),
        )
    }
}

// MARK: Exit fade

/**
 * Draws the children of [parent] (a box or a gesture) keyed by place, and keeps a child the tree dropped that carries
 * [JsModifier.ExitFade] as it last drew, fading out with CSS's `ease`, under the same key so its state stays with it.
 * A child leaves when the new children hold no child of its kind with the same fade. The parent is passed whole, as
 * the other widget composables take theirs, so the composable's parameters stay stable.
 */
@Composable
internal fun ExitFadeChildren(parent: JsWidget, content: @Composable (JsWidget) -> Unit) {
    val children = when (parent) {
        is JsWidget.Box -> parent.children
        is JsWidget.Gesture -> parent.children
        else -> emptyList()
    }
    val exits = remember { ExitFades() }
    exits.update(children, ValueAnimator.areAnimatorsEnabled())
    // Read so a finished fade, which bumps it, redraws the children without the one that left.
    @Suppress("UNUSED_VARIABLE", "unused")
    val finished = exits.finished.intValue
    val slots = (children.mapIndexed { slot, widget -> Slot(slot, widget, null) } + exits.leaving.map { Slot(it.slot, it.widget, it) })
        .sortedBy { it.index }
    slots.forEach { slot ->
        key(slot.index, slot.widget::class) {
            val exit = slot.exit
            if (exit == null) {
                content(slot.widget)
            } else {
                val alpha = remember(exit) { Animatable(1f) }
                LaunchedEffect(exit) {
                    alpha.animateTo(0f, tween(exit.durationMs, easing = WidgetStyleRules.CssEase))
                    exits.leaving.remove(exit)
                    exits.finished.intValue += 1
                }
                // The exit layer must blend with the backdrop outside its alpha layer.
                val blending = slot.widget.modifiers().filterIsInstance<JsModifier.BlendingMode>().toComposeModifier()
                Box(blending.graphicsLayer { this.alpha = alpha.value }) { content(slot.widget) }
            }
        }
    }
}

private class Slot(val index: Int, val widget: JsWidget, val exit: Leaving?)

internal class Leaving(val slot: Int, val widget: JsWidget, val durationMs: Int)

/** The children last drawn and the ones fading out; plain fields, so composing updates them without a redraw loop. */
private class ExitFades {
    var previous: List<JsWidget>? = null
    val leaving = mutableListOf<Leaving>()
    val finished = mutableIntStateOf(0)

    fun update(children: List<JsWidget>, animate: Boolean) {
        val before = previous
        previous = children
        if (before == null || before === children || !animate) return
        before.forEachIndexed { index, old ->
            val fade = old.exitFade() ?: return@forEachIndexed
            val stays = children.any { it::class == old::class && it.exitFade() == fade }
            if (!stays && leaving.none { it.slot == index }) leaving.add(Leaving(index, old, fade.durationMs))
        }
        // A child that came back is drawn again, not faded.
        leaving.removeAll { exit -> children.any { it::class == exit.widget::class && it.exitFade()?.durationMs == exit.durationMs } }
    }
}

private fun JsWidget.exitFade(): JsModifier.ExitFade? = modifiers().filterIsInstance<JsModifier.ExitFade>().firstOrNull()

internal fun JsWidget.modifiers(): List<JsModifier> = when (this) {
    is JsWidget.Box -> modifiers
    is JsWidget.Column -> modifiers
    is JsWidget.Row -> modifiers
    is JsWidget.Spacer -> modifiers
    is JsWidget.Text -> modifiers
    is JsWidget.Button -> modifiers
    is JsWidget.Gesture -> modifiers
    is JsWidget.TextField -> modifiers
    is JsWidget.Image -> modifiers
    is JsWidget.Materials -> emptyList()
}

/** The shape a node's shadow takes: its fill's, else its background's, else its box. */
@Composable
internal fun List<JsModifier>.shadowShape(): Shape =
    (filterIsInstance<JsModifier.Fill>().firstOrNull()?.shape ?: filterIsInstance<JsModifier.Background>().firstOrNull()?.shape)
        ?.toComposeShape() ?: RectangleShape

private const val FULL_TURN = 360f
private const val MAX_WEIGHT = 1000
