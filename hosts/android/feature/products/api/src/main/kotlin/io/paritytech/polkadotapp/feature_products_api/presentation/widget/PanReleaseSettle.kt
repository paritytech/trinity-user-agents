package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.ui.geometry.Offset
import io.paritytech.polkadotapp.feature_products_api.model.JsPanAxis

/**
 * How the children of an every-move pan (a keyed pan the worker moves, with no host drag) settle when the finger lifts,
 * instead of jumping in one frame when the worker's end tree arrives: home with the host's return spring, or to the end
 * with a short ease-out once released at or past 90% of the travel. The worker's next tree then decides where they
 * stay. While the finger is down the children follow the worker's answers directly; nothing here runs then. A new drag
 * cuts the settle short, and with animations removed (Settings, Accessibility) the children go straight there. The
 * `debug.gesture.settle` property (`0` turns it off, read once when the app starts) is for measuring without it.
 */
internal object PanReleaseSettle {
    enum class Kind { HOME, END }

    /** A release at or past this share of the travel settles to the end: 90 %. */
    const val COMPLETION_PERMILLE = 900

    /** The ease-out to the end, in ms. */
    const val END_MILLIS = 140

    fun isOn(setting: String?): Boolean = GesturePacingSettings.sendOnTouch(setting)

    val currentIsOn: Boolean = isOn(GesturePacingSettings.property("debug.gesture.settle"))

    /** Where released children go: the end only when released past [COMPLETION_PERMILLE] along a single axis. */
    fun kind(axis: JsPanAxis, releasedPermille: Int, cancelled: Boolean): Kind =
        if (!cancelled && axis != JsPanAxis.FREE && releasedPermille >= COMPLETION_PERMILLE) Kind.END else Kind.HOME

    /** The settle's end, in px from the travel's start. */
    fun target(kind: Kind, axis: JsPanAxis, travelPx: Float): Offset = when (kind) {
        Kind.HOME -> Offset.Zero
        Kind.END -> if (axis == JsPanAxis.VERTICAL) Offset(0f, travelPx) else Offset(travelPx, 0f)
    }

    /**
     * Where settling children are drawn: kept on their lane, so the spring's overshoot past home (or the end) is not
     * drawn. A release home with a feel's spring may dip below home, as an underdamped spring does (about 1.7 dp from a
     * mid-lane release at 18 rad/s and 0.8); it still never passes the end.
     */
    fun onLane(place: Offset, travelPx: Float, dipsBelowHome: Boolean = false): Offset =
        Offset(along(place.x, travelPx, dipsBelowHome), along(place.y, travelPx, dipsBelowHome))

    /** One axis of [onLane]: between home and the end, or only short of the end while a release home may dip. */
    fun along(place: Float, travelPx: Float, dipsBelowHome: Boolean = false): Float {
        val lane = travelPx.coerceAtLeast(0f)
        return if (dipsBelowHome) place.coerceAtMost(lane) else place.coerceIn(0f, lane)
    }

    /** A settle is drawn only with animations on and somewhere to go. */
    fun animates(animatorsEnabled: Boolean, from: Offset, to: Offset): Boolean = animatorsEnabled && from != to
}
