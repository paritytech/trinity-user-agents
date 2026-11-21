package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsSlideToConfirmVariant
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import kotlin.math.abs

internal enum class SlideDragDecision { CLAIM, WAIT, RELEASE }

internal enum class SlideSettle { PIN_AT_END, RETURN_TO_START, STAY }

/** The slide's rules, kept apart from drawing and pointer plumbing so they can be checked on their own. */
internal object SlideToConfirmRules {
    const val CONFIRM_THRESHOLD = 0.9f

    /** The thumb variant's own track height, in dp, for a node that gives none. */
    const val THUMB_TRACK_HEIGHT_DP = 72

    /** The thumb's diameter, in dp: the least a track can stand, since it has to hold the thumb. */
    const val THUMB_SIZE_DP = 56

    /**
     * How tall the thumb variant's track stands, in dp: the height the product gave the node, such as
     * the room footer's 80 that every docked card shares, or the track's own 72. Compose sizes a node
     * by the first height it is given, so that one counts.
     */
    fun thumbTrackHeightDp(widget: JsWidget.SlideToConfirm): Int =
        (widget.modifiers.firstNotNullOfOrNull { (it as? JsModifier.Size)?.height } ?: THUMB_TRACK_HEIGHT_DP)
            .coerceAtLeast(THUMB_SIZE_DP)

    /** The space around the thumb inside the track, in dp, the same on every side: 8 in the track's own 72, 12 in 80. */
    fun thumbInsetDp(widget: JsWidget.SlideToConfirm): Float = (thumbTrackHeightDp(widget) - THUMB_SIZE_DP) / 2f

    /** What a full slide keeps from each side of the control, in dp: the thumb's inset, or nothing for the reveal's peel. */
    fun travelSideInsetDp(widget: JsWidget.SlideToConfirm): Float = when (widget.variant) {
        JsSlideToConfirmVariant.THUMB -> thumbInsetDp(widget)
        JsSlideToConfirmVariant.REVEAL -> 0f
    }

    /**
     * How far a full slide goes, in px, on a control [widthPx] wide. The drag divides the finger's move by this
     * and the face draws the thumb over the same distance, so the thumb follows the finger one for one. The thumb
     * travels the lane inside the track's inset ([sideInsetPx] a side) less its own width ([endInsetPx]).
     */
    fun travelPx(widthPx: Float, endInsetPx: Float, sideInsetPx: Float): Float =
        (widthPx - endInsetPx - 2 * sideInsetPx).coerceAtLeast(0f)

    /** A control without an action is inert, as a `Button` without one is. */
    fun acceptsSlide(widget: JsWidget.SlideToConfirm, heldAtEnd: Boolean): Boolean =
        widget.enabled && !widget.loading && !widget.confirmed && widget.onConfirm != null && !heldAtEnd

    /**
     * A drag past the slop that is mostly sideways is the control's in either direction, so swipe to
     * reply never starts on it. Mostly up or down, it is the surface's, such as the feed's scroll.
     */
    fun decide(dx: Float, dy: Float, slop: Float): SlideDragDecision = when {
        abs(dx) > slop && abs(dx) >= abs(dy) -> SlideDragDecision.CLAIM
        abs(dy) > slop -> SlideDragDecision.RELEASE
        else -> SlideDragDecision.WAIT
    }

    fun slideBy(progress: Float, dx: Float, travelPx: Float, rtl: Boolean): Float =
        if (travelPx <= 0f) progress else (progress + (if (rtl) -dx else dx) / travelPx).coerceIn(0f, 1f)

    fun completes(progress: Float, lifted: Boolean): Boolean = lifted && progress >= CONFIRM_THRESHOLD

    /** Loading or confirmed pins the control at the end; a tree that clears both after either was set sends it back. */
    fun settle(heldAtEnd: Boolean, loading: Boolean, confirmed: Boolean): SlideSettle = when {
        loading || confirmed -> SlideSettle.PIN_AT_END
        heldAtEnd -> SlideSettle.RETURN_TO_START
        else -> SlideSettle.STAY
    }
}
