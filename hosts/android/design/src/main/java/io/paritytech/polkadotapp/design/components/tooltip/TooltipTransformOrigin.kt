package io.paritytech.polkadotapp.design.components.tooltip

import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.unit.IntRect
import kotlin.math.max
import kotlin.math.min

internal fun calculateTooltipTransformOrigin(
    anchorBounds: IntRect,
    tooltipBounds: IntRect
): TransformOrigin {
    val pivotX = when {
        tooltipBounds.left >= anchorBounds.right -> 0f
        tooltipBounds.right <= anchorBounds.left -> 1f
        tooltipBounds.width == 0 -> 0f
        else -> {
            val intersectionCenter =
                (
                    max(anchorBounds.left, tooltipBounds.left) +
                        min(anchorBounds.right, tooltipBounds.right)
                    ) / 2
            (intersectionCenter - tooltipBounds.left).toFloat() / tooltipBounds.width
        }
    }
    val pivotY = when {
        tooltipBounds.top >= anchorBounds.bottom -> 0f
        tooltipBounds.bottom <= anchorBounds.top -> 1f
        tooltipBounds.height == 0 -> 0f
        else -> {
            val intersectionCenter =
                (
                    max(anchorBounds.top, tooltipBounds.top) +
                        min(anchorBounds.bottom, tooltipBounds.bottom)
                    ) / 2
            (intersectionCenter - tooltipBounds.top).toFloat() / tooltipBounds.height
        }
    }
    return TransformOrigin(pivotX, pivotY)
}
