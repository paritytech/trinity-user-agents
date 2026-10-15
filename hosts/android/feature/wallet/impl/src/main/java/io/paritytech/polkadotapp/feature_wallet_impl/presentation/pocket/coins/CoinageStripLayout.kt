package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import kotlin.math.PI
import kotlin.math.acos
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * The summary strip: every coin at a fixed height, in a fixed width.
 *
 * Ported line for line from the strip layout in the maintainers' internal coinage reference implementation
 * and checked against its conformance vectors, so the two stay the same strip.
 *
 * As coins are added the strip gives ground in a fixed order. First the margins close, from ten points
 * toward four, which is why ten coins keep their full margin and nothing jumps at the coin that first does
 * not fit. Then the coins turn about their vertical axis, the face shrinking as cosine and the edge growing
 * as sine, solved exactly for the width rather than stepped. Fully edge-on they get thinner, which is how a
 * thousand coins fit in a thousand points at one point each.
 */
object CoinageStripLayout {
    enum class Partition(val key: String) {
        CLEARING("clearing"),
        READY("ready")
    }

    data class Options(
        val height: Float = 60f,
        val margin: Float = 10f,
        val minimumMargin: Float = 4f,
        /**
         * Between the Clearing and Ready runs, face on. Twice a margin and then some: it is the only thing
         * separating the two, now that the bar above them is gone.
         */
        val gap: Float = 26f,
        val minimumGap: Float = 8f
    )

    /** One coin's design, as the strip needs it. All three are in coin heights. */
    data class Coin(
        /** Diameter as a fraction of the strip height. */
        val size: Float,
        /** Face-on extent. */
        val width: Float,
        val thickness: Float,
        val partition: Partition
    )

    data class Placement(
        val centreX: Float,
        val centreY: Float,
        val height: Float,
        val turn: Float,
        val thickness: Float,
        /** What the coin covers horizontally once turned. */
        val visible: Float
    )

    /** A run of one partition, for the label under it. */
    data class Span(
        val partition: Partition,
        val start: Float,
        val end: Float,
        val count: Int
    )

    data class Layout(
        val turn: Float,
        val margin: Float,
        val gap: Float,
        /** How far coins had to be thinned past 90 degrees. `1` is their own thickness. */
        val thinning: Float,
        val coins: List<Placement>,
        val partitions: List<Span>,
        /** Total width taken, which is what the strip actually fills. */
        val used: Float
    )

    fun layout(coins: List<Coin>, width: Float, options: Options = Options()): Layout {
        if (coins.isEmpty()) {
            return Layout(
                turn = 0f,
                margin = options.margin,
                gap = options.gap,
                thinning = 1f,
                coins = emptyList(),
                partitions = emptyList(),
                used = 0f
            )
        }

        return place(coins, solve(totals(coins, options), width, options), options)
    }

    /**
     * How calm the shading of a nearly edge-on coin should be, so it reads as a clean band of colour rather
     * than as a lit disc seen from the side.
     */
    fun edgeCalm(turn: Float): Float {
        val progress = ((kotlin.math.abs(turn) - 60f * DEGREE) / (25f * DEGREE)).coerceIn(0f, 1f)

        return progress * progress * (3f - 2f * progress)
    }

    /**
     * Solved in double precision, as the reference does. The figures accumulate once per coin and the strip
     * takes a thousand of them, which is far enough for single precision to drift off the conformance
     * vectors — a third of a hundredth of a point at a thousand coins, invisible on screen but exactly the
     * kind of quiet divergence the vectors exist to catch.
     */
    private data class Totals(
        val faces: Double,
        val edges: Double,
        val margins: Int,
        val boundaries: Int
    )

    private data class Solution(
        val turn: Double,
        val margin: Double,
        val gap: Double,
        val thinning: Double
    )

    private fun totals(coins: List<Coin>, options: Options): Totals {
        val boundaries = coins.zipWithNext().count { (left, right) -> left.partition != right.partition }
        val height = options.height.toDouble()

        return Totals(
            faces = coins.fold(0.0) { running, coin -> running + height * coin.size * coin.width },
            edges = coins.fold(0.0) { running, coin -> running + height * coin.size * coin.thickness },
            margins = coins.size - 1 - boundaries,
            boundaries = boundaries
        )
    }

    private fun solve(totals: Totals, width: Float, options: Options): Solution {
        val margin = options.margin.toDouble()
        val gap = options.gap.toDouble()
        val minimumMargin = options.minimumMargin.toDouble()
        val faceOn = totals.faces + totals.margins * margin + totals.boundaries * gap

        if (faceOn <= width) {
            return Solution(turn = 0.0, margin = margin, gap = gap, thinning = 1.0)
        }

        val room = width - totals.faces
        val full = totals.margins * margin + totals.boundaries * gap
        val least = totals.margins * minimumMargin + totals.boundaries * gap

        if (room >= least && full > 0.0) {
            val squeezed = if (totals.margins > 0) {
                (room - totals.boundaries * gap) / totals.margins
            } else {
                0.0
            }

            return Solution(
                turn = 0.0,
                margin = maxOf(minimumMargin, squeezed),
                gap = gap,
                thinning = 1.0
            )
        }

        return turned(totals, width, options)
    }

    /**
     * `A·cos θ + B·sin θ = C`, where A is everything that shrinks as the coins turn and B is the edge that
     * grows in its place. Past the point where even a wall of edges is too wide, the angle stops at 90
     * degrees and the coins thin instead.
     */
    private fun turned(totals: Totals, width: Float, options: Options): Solution {
        val minimumMargin = options.minimumMargin.toDouble()
        val minimumGap = options.minimumGap.toDouble()
        val gap = options.gap.toDouble()
        val shrinking = totals.faces + totals.margins * minimumMargin + totals.boundaries * (gap - minimumGap)
        val growing = totals.edges
        val available = width - totals.boundaries * minimumGap
        val radius = sqrt(shrinking * shrinking + growing * growing)

        val turn: Double
        val thinning: Double

        if (growing >= available) {
            turn = PI / 2
            thinning = available / growing
        } else {
            turn = minOf(PI / 2, atan2(growing, shrinking) + acos(minOf(1.0, available / radius)))
            thinning = 1.0
        }

        return Solution(
            turn = turn,
            margin = minimumMargin * cos(turn),
            gap = minimumGap + (gap - minimumGap) * cos(turn),
            thinning = thinning
        )
    }

    private fun place(coins: List<Coin>, solved: Solution, options: Options): Layout {
        val cosine = cos(solved.turn)
        val sine = sin(solved.turn)
        val stripHeight = options.height.toDouble()

        val placements = ArrayList<Placement>(coins.size)
        val partitions = ArrayList<Span>()
        var cursor = 0.0

        coins.forEachIndexed { index, coin ->
            if (index > 0) {
                cursor += if (coins[index - 1].partition != coin.partition) solved.gap else solved.margin
            }

            val height = stripHeight * coin.size
            val thickness = coin.thickness * solved.thinning
            val visible = height * (coin.width * cosine + thickness * sine)

            placements += Placement(
                centreX = (cursor + visible / 2).toFloat(),
                centreY = (stripHeight / 2).toFloat(),
                height = height.toFloat(),
                turn = solved.turn.toFloat(),
                thickness = thickness.toFloat(),
                visible = visible.toFloat()
            )

            val last = partitions.lastOrNull()

            if (last != null && last.partition == coin.partition) {
                partitions[partitions.lastIndex] =
                    last.copy(end = (cursor + visible).toFloat(), count = last.count + 1)
            } else {
                partitions += Span(
                    partition = coin.partition,
                    start = cursor.toFloat(),
                    end = (cursor + visible).toFloat(),
                    count = 1
                )
            }

            cursor += visible
        }

        return Layout(
            turn = solved.turn.toFloat(),
            margin = solved.margin.toFloat(),
            gap = solved.gap.toFloat(),
            thinning = solved.thinning.toFloat(),
            coins = placements,
            partitions = partitions,
            used = cursor.toFloat()
        )
    }
}

private const val DEGREE = (PI / 180).toFloat()
