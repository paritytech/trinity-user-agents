package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.LayoutCoordinates

/** Resolves layout coordinates on the UI thread; a render-stream tree reads only the immutable projection. */
internal class MaskCoordinates {
    var node: LayoutCoordinates? = null
    var projection by mutableStateOf<MaskProjection?>(null)
        private set

    fun update(pan: LayoutCoordinates?) {
        val target = node ?: return
        if (pan?.isAttached != true || !target.isAttached) return
        val origin = target.localPositionOf(pan, Offset.Zero)
        val horizontal = target.localPositionOf(pan, Offset(1f, 0f)) - origin
        val vertical = target.localPositionOf(pan, Offset(0f, 1f)) - origin
        // A fully collapsed node has no inverse transform.
        projection = if (listOf(origin, horizontal, vertical).all { it.x.isFinite() && it.y.isFinite() }) {
            MaskProjection(origin, horizontal, vertical)
        } else {
            null
        }
    }
}

internal data class MaskProjection(val origin: Offset, val horizontal: Offset, val vertical: Offset) {
    fun point(at: Offset): Offset = origin + horizontal * at.x + vertical * at.y
}
