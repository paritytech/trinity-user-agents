package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.ui.geometry.Offset

/** Where a finger presses a gesture's children, for the feel's press scale. */
internal object PanPress {
    /** How far around the drawn children a finger still presses them. */
    const val REACH_DP = 10f

    /** Whether a finger at [at] presses children drawn at [start], [top], [width] by [height], with [reachPx] around. */
    fun reaches(at: Offset, start: Float, top: Float, width: Float, height: Float, reachPx: Float): Boolean =
        at.x >= start - reachPx && at.x <= start + width + reachPx && at.y >= top - reachPx && at.y <= top + height + reachPx
}
