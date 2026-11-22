package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.ui.geometry.Offset
import io.paritytech.polkadotapp.feature_products_api.model.JsPanAxis

/**
 * How an every-move worker thumb (a keyed pan with no host drag) settles when the finger lifts, instead of jumping in
 * one frame: home with the host's return spring, or to the end with a short ease-out once the drag passed the worker's
 * completion point. While the finger is down the thumb follows the worker's answers directly; nothing here runs then.
 * A new drag cuts the settle short, and with animations removed (Settings, Accessibility) the thumb goes straight there.
 * The `debug.gesture.settle` property (`0` turns it off, read once when the app starts) is for measuring without it.
 */
internal object WorkerThumbSettle {
    enum class Kind { HOME, END }

    /** The worker's own completion rule: 90 % of the travel. */
    const val COMPLETION_PERMILLE = 900

    /** The ease-out to the end, in ms. */
    const val END_MILLIS = 140

    fun isOn(setting: String?): Boolean = GesturePacingSettings.sendOnTouch(setting)

    val currentIsOn: Boolean = isOn(GesturePacingSettings.property("debug.gesture.settle"))

    /** Where a released thumb goes: the end only when released past the completion point along a single axis. */
    fun kind(axis: JsPanAxis, releasedPermille: Int, cancelled: Boolean): Kind =
        if (!cancelled && axis != JsPanAxis.FREE && releasedPermille >= COMPLETION_PERMILLE) Kind.END else Kind.HOME

    /** The settle's end, in px from the travel's start. */
    fun target(kind: Kind, axis: JsPanAxis, travelPx: Float): Offset = when (kind) {
        Kind.HOME -> Offset.Zero
        Kind.END -> if (axis == JsPanAxis.VERTICAL) Offset(0f, travelPx) else Offset(travelPx, 0f)
    }

    /**
     * Where a settling thumb is drawn: kept on its lane, so the spring's overshoot past home (or the end) is not drawn,
     * as the native slider and host drags draw their return spring.
     */
    fun onLane(place: Offset, travelPx: Float): Offset {
        val lane = travelPx.coerceAtLeast(0f)
        return Offset(place.x.coerceIn(0f, lane), place.y.coerceIn(0f, lane))
    }

    /** A settle is drawn only with animations on and somewhere to go. */
    fun animates(animatorsEnabled: Boolean, from: Offset, to: Offset): Boolean = animatorsEnabled && from != to
}
