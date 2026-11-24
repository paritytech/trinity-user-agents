package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.Serializable
import kotlinx.serialization.Transient

/**
 * When a cover counts as cleared (`CoverGrid`'s `reveal`): once its cleared cells reach [atPermille] thousandths of the
 * grid and the finger is up on the pan keyed [pan], the host sends [action] once for that lift.
 * @param touch That pan's finger
 */
@Serializable
data class JsCoverReveal(
    val pan: String,
    val atPermille: Int,
    val action: String,
    @Transient val touch: JsGesturePosition? = null,
) {
    /** Whether [cleared] cells of [cells] reach the share, as the core counts it: `cleared * 1000 >= atPermille * cells`. */
    fun reached(cleared: Int, cells: Int): Boolean = cleared.toLong() * 1000 >= atPermille.toLong() * cells
}

/**
 * Decides when a cover's reveal is sent: after a lift, the first time its cleared cells reach the share, so the cells a
 * worker clears in answer to the pan's end count; never while the finger is down; once per lift.
 * @param lifts The lifts seen before the cover was drawn, which must not send its reveal
 */
class CoverRevealWatch(private var lifts: Long = 0) {
    private var armed = false

    /** Whether to send the reveal now. */
    fun update(touch: JsGesturePosition.Touch, cleared: Int, cells: Int, reveal: JsCoverReveal): Boolean {
        if (touch.lifts != lifts) {
            lifts = touch.lifts
            armed = true
        }
        if (touch.down) {
            armed = false
            return false
        }
        if (!armed || !reveal.reached(cleared, cells)) return false
        armed = false
        return true
    }
}
