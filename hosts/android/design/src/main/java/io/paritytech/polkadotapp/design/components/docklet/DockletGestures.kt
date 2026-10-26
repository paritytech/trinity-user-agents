package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import kotlin.math.max
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

enum class DockletStep { Rest, Step1, Step2 }

sealed interface DockletRelease {
    data object Open : DockletRelease
    data object Close : DockletRelease
    data class SpringBack(val step: DockletStep) : DockletRelease
}

object DockletGestures {
    val DragSlop = 10.dp
    val TapSlop = 14.dp

    private val MinSwipeTravel = 120.dp
    private const val RubberBand = 1f / 3f
    private const val ReleaseThreshold = 0.5f
    private const val FlickSpeed = 500f
    private val FlickWindow = 100.milliseconds
    private const val FastReleaseSpeed = 300f
    private const val ReleaseDurationFactor = 2.5f
    private val MinReleaseDuration = 180.milliseconds
    private val MaxReleaseDuration = 420.milliseconds
    private val SlowReleaseDuration = 340.milliseconds
    private const val KeyboardRequestShare = 0.6f

    fun swipeUpStarted(drag: DpOffset): Boolean = -drag.y > DragSlop

    fun isTap(moved: Dp): Boolean = moved < TapSlop

    // A system cancel arrives as an up that is already consumed.
    fun isRelease(upConsumed: Boolean, ownsGesture: Boolean, dragging: Boolean): Boolean =
        !(upConsumed && (ownsGesture || dragging))

    fun swipeProgress(distance: Dp, travel: Dp): Float {
        val raw = distance.value / max(travel.value, MinSwipeTravel.value)
        return when {
            raw > 1f -> 1f + (raw - 1f) * RubberBand
            raw < 0f -> raw * RubberBand
            else -> raw
        }
    }

    fun closedFraction(dragged: Float, travel: Float): Float = (dragged / travel).coerceIn(0f, 1f)

    fun releaseSwipeUp(progress: Float, upwardDpPerSecond: Float, travel: Dp): DockletRelease {
        if (progress >= ReleaseThreshold) return DockletRelease.Open

        val shortfall = (ReleaseThreshold - progress) * max(travel.value, MinSwipeTravel.value)
        val reachesHalfInTime = upwardDpPerSecond > FlickSpeed && (shortfall / upwardDpPerSecond).toDouble().seconds <= FlickWindow
        return if (reachesHalfInTime) DockletRelease.Open else DockletRelease.Close
    }

    fun releaseSwipeDown(closedFraction: Float, downwardDpPerSecond: Float, from: DockletStep): DockletRelease =
        if (closedFraction >= ReleaseThreshold || downwardDpPerSecond > FlickSpeed) {
            DockletRelease.Close
        } else {
            DockletRelease.SpringBack(from)
        }

    fun remainingMotion(remaining: Dp, dpPerSecond: Float): Duration {
        if (dpPerSecond <= FastReleaseSpeed) return SlowReleaseDuration

        return (ReleaseDurationFactor * remaining.value / dpPerSecond).toDouble().seconds
            .coerceIn(MinReleaseDuration, MaxReleaseDuration)
    }

    fun keyboardRequestAt(remainingMotion: Duration): Duration = remainingMotion * KeyboardRequestShare.toDouble()
}
