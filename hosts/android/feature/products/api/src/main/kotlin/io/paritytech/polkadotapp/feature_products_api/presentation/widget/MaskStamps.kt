package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsMaskBrush
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskBrushTip
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskPoint
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.sin

/** The core brush's spacing and tip geometry, in hundredths of a dp, shared by drawing and coverage. */
internal object MaskStamps {
    /** The most stamps one line has (`MASK_STAMPS_MAX`). */
    const val MAX_STAMPS = 4096

    /** The least distance between two stamps, in hundredths of a dp (`MASK_STAMP_SPACING_MIN`). */
    const val MIN_SPACING = 50.0

    data class Centre(val x: Double, val y: Double)

    fun centres(width: Long, brush: JsMaskBrush, points: List<JsMaskPoint>): List<Centre> {
        if (points.isEmpty() || width <= 0) return emptyList()
        val total = points.zipWithNext().sumOf { (a, b) -> hypot(b.x.toDouble() - a.x, b.y.toDouble() - a.y) }
        // Never closer than the core's least spacing, and stretched so a long line keeps to the most stamps.
        val spacing = maxOf(width * brush.spacingPermille.coerceIn(50, 4000) / 1000.0, MIN_SPACING, total / (MAX_STAMPS - 1))
        val centres = mutableListOf(Centre(points.first().x.toDouble(), points.first().y.toDouble()))
        var sinceLast = 0.0
        for ((a, b) in points.zipWithNext()) {
            val dx = b.x.toDouble() - a.x
            val dy = b.y.toDouble() - a.y
            val length = hypot(dx, dy)
            if (length == 0.0) continue
            var along = spacing - sinceLast
            while (along <= length && centres.size < MAX_STAMPS) {
                val t = along / length
                centres += Centre(a.x + t * dx, a.y + t * dy)
                along += spacing
            }
            sinceLast = length - (along - spacing)
        }
        if (sinceLast > 0.0 && centres.size < MAX_STAMPS) centres += Centre(points.last().x.toDouble(), points.last().y.toDouble())
        return centres
    }

    fun covers(width: Long, brush: JsMaskBrush, centre: Centre, x: Double, y: Double): Boolean {
        if (width <= 0) return false
        val halfLength = width / 2.0
        val halfThickness = halfLength * brush.aspectPermille.coerceIn(1, 1000) / 1000.0
        val angle = Math.toRadians(brush.angleCentideg / 100.0)
        val dx = x - centre.x
        val dy = y - centre.y
        val along = dx * cos(angle) + dy * sin(angle)
        val across = -dx * sin(angle) + dy * cos(angle)
        return when (brush.tip) {
            JsMaskBrushTip.ELLIPSE -> square(along / halfLength) + square(across / halfThickness) <= 1.0
            JsMaskBrushTip.CAPSULE -> {
                val core = max(halfLength - halfThickness, 0.0)
                square(max(abs(along) - core, 0.0)) + square(across) <= square(halfThickness)
            }
        }
    }

    private fun square(value: Double) = value * value
}
