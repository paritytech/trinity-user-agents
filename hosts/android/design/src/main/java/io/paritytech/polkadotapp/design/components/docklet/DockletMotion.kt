package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.Easing
import androidx.compose.animation.core.TweenSpec
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Immutable
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

@Immutable
data class DockletContentsFrame(val alpha: Float, val rise: Dp, val blurSigma: Dp)

object DockletMotion {
    val MorphDuration = 329.milliseconds
    val ContentsInDelay = 224.milliseconds
    val ContentsInDuration = 141.milliseconds
    val ContentsOutDuration = 120.milliseconds
    val HighlightDuration = 238.milliseconds
    val CrossFadeDuration = 220.milliseconds
    val PressInDuration = 77.milliseconds
    val PressOutDuration = 221.milliseconds
    val MinPressHold = 150.milliseconds
    const val PressedScale = 0.96f

    val HeightEasing = Easing { heightProgress(it) }
    val WidthEasing = Easing { widthProgress(it) }
    val CloseEasing = Easing { 1f - closeOutline((ContentsOutDuration + MorphDuration) * it.toDouble()) }
    val ContentsEasing = CubicBezierEasing(0.2f, 0.8f, 0.2f, 1f)
    val ContentsInEasing = Easing { contentsIn((ContentsInDelay + ContentsInDuration) * it.toDouble()).alpha }
    val ContentsOutEasing = Easing { 1f - contentsOutAlpha(ContentsOutDuration * it.toDouble()) }
    val HighlightEasing = CubicBezierEasing(0.25f, 0.1f, 0.25f, 1f)

    fun <T> stepChangeSpec(): TweenSpec<T> = tween(MorphDuration.inWholeMilliseconds.toInt(), easing = HeightEasing)

    private const val WidthDelay = 0.04f
    private const val WidthSmoothstepShare = 0.35f
    private const val SwipeContentsFrom = 0.88f
    private val ContentsRise = 8.dp
    private val ContentsBlur = 8.dp

    fun easeOut(t: Float): Float {
        val inverse = 1f - t.coerceIn(0f, 1f)
        return 1f - inverse * inverse * inverse
    }

    fun heightProgress(t: Float): Float = easeOut(t)

    fun widthProgress(t: Float): Float {
        val u = ((t - WidthDelay) / (1f - WidthDelay)).coerceIn(0f, 1f)
        val smoothstep = u * u * (3f - 2f * u)
        return WidthSmoothstepShare * smoothstep + (1f - WidthSmoothstepShare) * easeOut(u)
    }

    fun contentsIn(sinceStepChange: Duration): DockletContentsFrame {
        val fade = ((sinceStepChange - ContentsInDelay) / ContentsInDuration).toFloat().coerceIn(0f, 1f)
        return contentsFrame(ContentsEasing.transform(fade))
    }

    fun contentsFrame(alpha: Float): DockletContentsFrame = DockletContentsFrame(
        alpha = alpha,
        rise = ContentsRise * (1f - alpha),
        blurSigma = ContentsBlur * (1f - alpha),
    )

    fun contentsOutAlpha(sinceCloseStart: Duration): Float =
        1f - easeOut((sinceCloseStart / ContentsOutDuration).toFloat())

    fun closeOutline(sinceCloseStart: Duration): Float {
        val t = ((sinceCloseStart - ContentsOutDuration) / MorphDuration).toFloat()
        return 1f - easeOut(t)
    }

    fun swipeContentsAlpha(swipeProgress: Float): Float =
        ((swipeProgress - SwipeContentsFrom) / (1f - SwipeContentsFrom)).coerceIn(0f, 1f)
}
