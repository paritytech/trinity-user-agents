package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsPanAxis
import io.paritytech.polkadotapp.feature_products_api.model.JsPanRelease
import io.paritytech.polkadotapp.feature_products_api.model.JsPanTravel
import kotlin.math.abs
import kotlin.math.roundToInt

/** Where a released host-owned drag goes. */
internal enum class GestureSettle { COMPLETE, SNAP_BACK, STAY }

/**
 * A gesture node's rules, kept apart from drawing and pointer plumbing so they can be checked on their own.
 * The claim and slop rules are #2099's slide to confirm's, along the node's axis.
 */
internal object GestureRules {
    const val PERMILLE = 1000

    /**
     * Past the slop, a drag along the node's axis is the node's in either direction, so swipe to reply and the
     * feed's scroll never start on it; across the axis it is the surface's. A free pan claims any drag.
     */
    fun decide(axis: JsPanAxis, dx: Float, dy: Float, slop: Float): SlideDragDecision = when (axis) {
        JsPanAxis.HORIZONTAL -> SlideToConfirmRules.decide(dx, dy, slop)
        JsPanAxis.VERTICAL -> SlideToConfirmRules.decide(dy, dx, slop)
        JsPanAxis.FREE -> if (abs(dx) > slop || abs(dy) > slop) SlideDragDecision.CLAIM else SlideDragDecision.WAIT
    }

    /**
     * How far, in px, the children may move: for [JsPanTravel.ParentWidth] the gesture node's own width less its
     * children's, never below zero.
     */
    fun travelPx(travel: JsPanTravel?, nodeWidthPx: Int, childrenWidthPx: Int, dpToPx: Float): Float = when (travel) {
        null, JsPanTravel.ParentWidth -> (nodeWidthPx - childrenWidthPx).coerceAtLeast(0).toFloat()
        is JsPanTravel.Points -> (travel.dp * dpToPx).coerceAtLeast(0f)
    }

    /** The child's place along the travel after moving [delta] from [start], clamped to the travel. */
    fun place(start: Float, delta: Float, travelPx: Float): Float = (start + delta).coerceIn(0f, travelPx.coerceAtLeast(0f))

    /** How far along the travel [placePx] is, in thousandths; zero travel reads zero. */
    fun permille(placePx: Float, travelPx: Float): Int =
        if (travelPx <= 0f) 0 else ((placePx / travelPx).coerceIn(0f, 1f) * PERMILLE).roundToInt()

    /** Where a released drag goes. Zero travel never completes; the threshold is clamped to 1 through 100. */
    fun settle(release: JsPanRelease, permille: Int, travelPx: Float): GestureSettle = when (release) {
        JsPanRelease.SnapBack -> GestureSettle.SNAP_BACK
        JsPanRelease.Stay -> GestureSettle.STAY
        is JsPanRelease.Complete ->
            if (travelPx > 0f && permille >= release.thresholdPercent.coerceIn(1, 100) * PERMILLE / 100) {
                GestureSettle.COMPLETE
            } else {
                GestureSettle.SNAP_BACK
            }
    }

    /** A move goes out once [intervalMs] has passed since the last one sent; zero sends every move. */
    fun movesNow(nowMs: Long, lastSentMs: Long?, intervalMs: Int): Boolean =
        lastSentMs == null || nowMs - lastSentMs >= intervalMs.coerceAtLeast(0)

    /** A node takes a gesture while enabled, and a drag held at the end takes no more. */
    fun accepts(enabled: Boolean, heldAtEnd: Boolean): Boolean = enabled && !heldAtEnd
}
