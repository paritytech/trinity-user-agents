package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsMaskPoint
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShape
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min

/**
 * How much of a node a mask's shapes cover, by the rule the core gives (`cleared_share`), in the wire's hundredths of
 * a dp: the node is laid with 2 by 2 dp squares from its top-start corner, a part square counting as whole, and a
 * square is covered when its centre lies in any shape (on the edge counts).
 */
internal object MaskCoverage {
    /** The side of a sample square, in hundredths of a dp. */
    const val SIDE = 200

    private const val HALF = SIDE / 2.0

    /** Covered samples, and all samples, of a node [width] by [height] hundredths of a dp. */
    fun count(shapes: List<JsMaskShape>, width: Int, height: Int): Pair<Int, Int> {
        val columns = samples(width)
        val rows = samples(height)
        val total = columns * rows
        if (shapes.any { it is JsMaskShape.All }) return total to total
        if (total == 0) return 0 to 0
        val covered = BooleanArray(total)

        fun mark(left: Double, top: Double, right: Double, bottom: Double, inside: (Double, Double) -> Boolean) {
            val i1 = last(right, columns) ?: return
            val j1 = last(bottom, rows) ?: return
            val i0 = first(left)
            val j0 = first(top)
            for (j in j0..j1) {
                for (i in i0..i1) {
                    if (inside(i * SIDE + HALF, j * SIDE + HALF)) covered[j * columns + i] = true
                }
            }
        }

        for (shape in shapes) {
            when (shape) {
                JsMaskShape.All -> Unit
                is JsMaskShape.Stamped -> {
                    val radius = shape.width / 2.0
                    for (centre in MaskStamps.centres(shape.width, shape.brush, shape.points)) {
                        mark(centre.x - radius, centre.y - radius, centre.x + radius, centre.y + radius) { x, y ->
                            MaskStamps.covers(shape.width, shape.brush, centre, x, y)
                        }
                    }
                }
                is JsMaskShape.Circle -> {
                    val cx = shape.center.x.toDouble()
                    val cy = shape.center.y.toDouble()
                    val r = shape.radius.toDouble()
                    mark(cx - r, cy - r, cx + r, cy + r) { x, y -> square(x - cx) + square(y - cy) <= r * r }
                }
                is JsMaskShape.Stroke -> {
                    val r = shape.width / 2.0
                    val points = shape.points
                    val segments = when (points.size) {
                        0 -> emptyList()
                        1 -> listOf(points[0] to points[0])
                        else -> points.zipWithNext()
                    }
                    for ((a, b) in segments) {
                        mark(
                            min(a.x, b.x) - r,
                            min(a.y, b.y) - r,
                            max(a.x, b.x) + r,
                            max(a.y, b.y) + r,
                        ) { x, y -> distanceSquared(x, y, a, b) <= r * r }
                    }
                }
            }
        }
        return covered.count { it } to total
    }

    private fun samples(length: Int): Int = (max(length, 0) + SIDE - 1) / SIDE

    private fun first(edge: Double): Int = max(ceil((edge - HALF) / SIDE), 0.0).toInt()

    private fun last(edge: Double, count: Int): Int? {
        val at = floor((edge - HALF) / SIDE)
        return if (at < 0) null else min(at.toInt(), count - 1)
    }

    private fun square(value: Double) = value * value

    /** From ([x], [y]) to the segment [a]-[b], squared. */
    private fun distanceSquared(x: Double, y: Double, a: JsMaskPoint, b: JsMaskPoint): Double {
        val dx = (b.x - a.x).toDouble()
        val dy = (b.y - a.y).toDouble()
        val length = dx * dx + dy * dy
        val t = if (length == 0.0) 0.0 else (((x - a.x) * dx + (y - a.y) * dy) / length).coerceIn(0.0, 1.0)
        return square(x - a.x - t * dx) + square(y - a.y - t * dy)
    }
}

/**
 * Decides when a mask's cleared share is checked, and so when its action may be sent.
 *
 * A mask that follows a pan is checked when the finger lifts, and again whenever shapes are added before it touches
 * down again, so shapes a worker adds in answer to the pan's end count; never while the finger is down; at most once
 * per lift. A mask with no pan to follow is checked whenever shapes are added, at most once per tree. A tree's own
 * shapes are never a reason to check.
 *
 * @param seen What happened before the mask was drawn, which must not send its action.
 */
internal class MaskShareWatch(private var seen: Input = Input()) {
    data class Input(
        /** Whether the followed pan's finger is down. */
        val down: Boolean = false,
        /** The followed pan's lifts so far. */
        val lifts: Int = 0,
        /** The trees this mask has seen. */
        val trees: Int = 0,
        /** How many times shapes were added to this mask. */
        val additions: Int = 0,
    )

    private var armed = false
    private var sentThisTree = false

    /** Whether to check the share now; [reached] is asked only then. */
    fun update(input: Input, follows: Boolean, reached: () -> Boolean): Boolean {
        val lifted = input.lifts != seen.lifts
        val added = input.additions != seen.additions
        if (input.trees != seen.trees) sentThisTree = false
        seen = input
        if (follows) {
            if (lifted) armed = true
            if (input.down) {
                armed = false
                return false
            }
            if (!armed || !(lifted || added) || !reached()) return false
            armed = false
            return true
        }
        if (!added || sentThisTree || !reached()) return false
        sentThisTree = true
        return true
    }
}
