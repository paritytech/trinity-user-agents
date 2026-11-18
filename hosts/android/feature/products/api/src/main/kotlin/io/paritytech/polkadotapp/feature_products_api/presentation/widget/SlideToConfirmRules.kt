package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import kotlin.math.abs

internal enum class SlideDragDecision { CLAIM, WAIT, RELEASE }

internal enum class SlideSettle { PIN_AT_END, RETURN_TO_START, STAY }

/** The slide's rules, kept apart from drawing and pointer plumbing so they can be checked on their own. */
internal object SlideToConfirmRules {
    const val CONFIRM_THRESHOLD = 0.9f

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
