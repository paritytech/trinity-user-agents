package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import kotlin.math.ceil
import kotlin.math.sqrt

/**
 * The coin grid: coins face on, packed in offset rows like a honeycomb, in the same order as the strip, one
 * block per partition under its header.
 *
 * Ported from `src/layout/hexgrid.js` and checked against the reference's own conformance vectors. The pitch
 * is as tight as fits; once even the smallest pitch will not hold every coin, the grid simply grows taller
 * and scrolls.
 *
 * The reference answers that case by stacking runs of alike coins into piles instead. That is not ported: a
 * stack of two reads as one oddly thick coin and answers a grouping question nobody asked.
 */
object CoinageGridLayout {
    data class Options(
        val maxDiameter: Float = 52f,
        /**
         * The reference's own floor, and what any crowded grid is drawn at: the fit is a cliff rather than a
         * slope, so a grid that overflows at all drops straight here. Fifty coins are drawn at 51 and two
         * hundred at the floor, with nothing in between.
         */
        val minDiameter: Float = 28f,
        val gap: Float = 5f,
        val header: Float = 34f,
        val blockGap: Float = 14f
    )

    /** One holding, as much of it as the packing needs. */
    data class Item(
        val id: String,
        val exponent: Int,
        val partition: CoinageStripLayout.Partition
    )

    data class Cell(val id: String, val centreX: Float, val centreY: Float)

    data class Block(
        val partition: CoinageStripLayout.Partition,
        val top: Float,
        val count: Int
    )

    data class Layout(
        val diameter: Float,
        val pitch: Float,
        val columns: Int,
        val cells: List<Cell>,
        val blocks: List<Block>,
        val height: Float,
        val fits: Boolean
    )

    fun layout(
        items: List<Item>,
        areaWidth: Float,
        areaHeight: Float,
        options: Options = Options()
    ): Layout {
        val partitions = split(items)

        // The search is over a monotone choice, so it bisects: the same answer as trying every option, in a
        // handful of passes rather than dozens.
        val diameters = (options.maxDiameter - options.minDiameter).toInt() + 1
        val widest = firstFit(diameters) {
            attempt(options.maxDiameter - it, partitions, areaWidth, areaHeight, options).fits
        }

        val diameter = if (widest < diameters) options.maxDiameter - widest else options.minDiameter

        return attempt(diameter, partitions, areaWidth, areaHeight, options)
    }

    private data class PartitionRun(
        val partition: CoinageStripLayout.Partition,
        val items: MutableList<Item>
    )

    private fun split(items: List<Item>): List<PartitionRun> =
        items.fold(mutableListOf<PartitionRun>()) { runs, item ->
            val last = runs.lastOrNull()

            if (last != null && last.partition == item.partition) {
                last.items += item
            } else {
                runs += PartitionRun(item.partition, mutableListOf(item))
            }

            runs
        }

    /** Smallest index in `0 until count` that fits, given fitting is monotone; `count` if none does. */
    private inline fun firstFit(count: Int, fits: (Int) -> Boolean): Int {
        var low = 0
        var high = count

        while (low < high) {
            val mid = (low + high) / 2

            if (fits(mid)) high = mid else low = mid + 1
        }

        return low
    }

    private fun attempt(
        diameter: Float,
        partitions: List<PartitionRun>,
        areaWidth: Float,
        areaHeight: Float,
        options: Options
    ): Layout {
        val pitch = diameter + options.gap
        val columns = maxOf(1, ((areaWidth - pitch / 2f + options.gap) / pitch).toInt())
        val rowHeight = pitch * SQRT_3 / 2f

        val cells = ArrayList<Cell>()
        val blocks = ArrayList<Block>()
        var top = 0f

        partitions.forEachIndexed { index, run ->
            if (index > 0) top += options.blockGap

            blocks += Block(run.partition, top = top, count = run.items.size)
            top += options.header

            val rows = ceil(run.items.size.toDouble() / columns).toInt()

            run.items.forEachIndexed { position, item ->
                val row = position / columns
                val column = position % columns

                cells += Cell(
                    id = item.id,
                    centreX = diameter / 2f + column * pitch + (row % 2) * (pitch / 2f),
                    centreY = top + diameter / 2f + row * rowHeight
                )
            }

            top += if (rows > 0) (rows - 1) * rowHeight + diameter else 0f
        }

        return Layout(
            diameter = diameter,
            pitch = pitch,
            columns = columns,
            cells = cells,
            blocks = blocks,
            height = top,
            fits = top <= areaHeight
        )
    }
}

private val SQRT_3 = sqrt(3f)
