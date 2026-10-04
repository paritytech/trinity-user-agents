package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.math.abs

/**
 * Checks the ported layout and motion against the vectors the maintainers' internal coinage reference
 * implementation exports, so the strip on the phone is the strip in the reference rather than something that
 * merely resembles it.
 *
 * The vectors are vendored verbatim under `resources/coinage/conformance` and shared with the iOS port, so a
 * drift between the two platforms shows up here rather than on a screen.
 */
class CoinageConformanceTest {
    @Test
    fun `strip solves the same turn margins and placements as the reference`() {
        val vectors = load<StripVectors>("strip")

        for (case in vectors.cases) {
            val layout = CoinageStripLayout.layout(
                case.input.map {
                    CoinageStripLayout.Coin(
                        size = it.size,
                        width = it.width,
                        thickness = it.thickness,
                        partition = partition(it.partition)
                    )
                },
                width = case.width,
                options = CoinageStripLayout.Options(
                    height = case.options.height,
                    margin = case.options.margin,
                    minimumMargin = case.options.minMargin,
                    gap = case.options.gap,
                    minimumGap = case.options.minGap
                )
            )

            near(case.label, "turn", case.output.turn, layout.turn)
            near(case.label, "margin", case.output.margin, layout.margin)
            near(case.label, "gap", case.output.gap, layout.gap)
            near(case.label, "thinning", case.output.thinning, layout.thinning)
            near(case.label, "used", case.output.used, layout.used)

            assertEquals("${case.label}: count", case.output.coins.size, layout.coins.size)

            layout.coins.zip(case.output.coins).forEach { (placed, expected) ->
                near(case.label, "x", expected.x, placed.centreX)
                near(case.label, "y", expected.y, placed.centreY)
                near(case.label, "height", expected.height, placed.height)
                near(case.label, "coin turn", expected.turn, placed.turn)
                near(case.label, "thickness", expected.thick, placed.thickness)
                near(case.label, "visible", expected.visible, placed.visible)
            }

            assertEquals("${case.label}: partitions", case.output.partitions.size, layout.partitions.size)

            layout.partitions.zip(case.output.partitions).forEach { (span, expected) ->
                assertEquals("${case.label}: partition", expected.partition, span.partition.key)
                assertEquals("${case.label}: span count", expected.count, span.count)
                near(case.label, "span from", expected.from, span.start)
                near(case.label, "span to", expected.to, span.end)
            }
        }
    }

    @Test
    fun `grid packs and blocks as the reference does`() {
        val vectors = load<GridVectors>("grid")

        assertEquals("no grid vectors", false, vectors.cases.isEmpty())

        for (case in vectors.cases) {
            val layout = CoinageGridLayout.layout(
                case.input.map {
                    CoinageGridLayout.Item(
                        id = it.id.toString(),
                        exponent = it.exponent,
                        partition = partition(it.partition)
                    )
                },
                areaWidth = case.area.width,
                areaHeight = case.area.height,
                options = CoinageGridLayout.Options(
                    maxDiameter = case.options.maxDiameter,
                    minDiameter = case.options.minDiameter,
                    gap = case.options.gap,
                    header = case.options.header,
                    blockGap = case.options.blockGap
                )
            )

            near(case.label, "diameter", case.output.diameter, layout.diameter)
            near(case.label, "pitch", case.output.pitch, layout.pitch)
            near(case.label, "height", case.output.height, layout.height)
            assertEquals("${case.label}: columns", case.output.cols, layout.columns)
            assertEquals("${case.label}: cells", case.output.cells.size, layout.cells.size)

            layout.cells.zip(case.output.cells).forEach { (cell, expected) ->
                near(case.label, "cell x", expected.x, cell.centreX)
                near(case.label, "cell y", expected.y, cell.centreY)
                assertEquals("${case.label}: cell id", expected.ids.map { it.toString() }, listOf(cell.id))
            }

            assertEquals("${case.label}: blocks", case.output.blocks.size, layout.blocks.size)

            layout.blocks.zip(case.output.blocks).forEach { (block, expected) ->
                assertEquals("${case.label}: block", expected.partition, block.partition.key)
                assertEquals("${case.label}: block count", expected.count, block.count)
                near(case.label, "block top", expected.y, block.top)
            }
        }
    }

    @Test
    fun `spring and edge calm match the reference`() {
        val vectors = load<MotionVectors>("motion")

        for (sample in vectors.spring) {
            val (value, velocity) = CoinageSpring.step(
                value = sample.input[0],
                velocity = sample.input[1],
                target = sample.input[2],
                seconds = sample.input[3],
                omega = sample.input[4]
            )

            near("spring", "value", sample.output[0], value)
            near("spring", "velocity", sample.output[1], velocity)
        }

        for (sample in vectors.edgeCalm) {
            near("edgeCalm", "calm", sample.calm, CoinageStripLayout.edgeCalm(sample.turn))
        }
    }

    @Test
    fun `fungibility ladder matches the reference`() {
        val vectors = load<FungibilityVectors>("fungibility")

        assertEquals("ring capacity", vectors.ringCapacity, CoinageWear.RING_CAPACITY)
        assertEquals("maximum level", vectors.maxLevel, CoinageWear.maximumLevel)

        for (sample in vectors.samples) {
            assertEquals("level for ${sample.k}", sample.level, CoinageWear.level(hiddenAmong = sample.k))
            near("wear", "level ${sample.level}", sample.wear, CoinageWear.amount(sample.level))
        }
    }

    private fun partition(key: String) = when (key) {
        CoinageStripLayout.Partition.CLEARING.key -> CoinageStripLayout.Partition.CLEARING
        else -> CoinageStripLayout.Partition.READY
    }

    /**
     * The vectors are printed to six decimal places, and every figure here is a length in points or an angle
     * in radians, so an absolute tolerance an order of magnitude below that is the right test.
     */
    private fun near(label: String, field: String, expected: Float, actual: Float) {
        if (abs(expected - actual) <= TOLERANCE) return

        assertEquals("$label: $field", expected.toDouble(), actual.toDouble(), TOLERANCE.toDouble())
    }

    private inline fun <reified T> load(name: String): T {
        val stream = checkNotNull(javaClass.getResourceAsStream("/coinage/conformance/$name.json")) {
            "missing conformance vectors for $name"
        }

        return json.decodeFromString<T>(stream.use { it.readBytes().toString(Charsets.UTF_8) })
    }

    private companion object {
        const val TOLERANCE = 1e-3f

        val json = Json { ignoreUnknownKeys = true }
    }

    @Serializable
    private data class StripVectors(val cases: List<StripCase>)

    @Serializable
    private data class StripCase(
        val label: String,
        val width: Float,
        val options: StripOptions,
        val input: List<StripCoin>,
        val output: StripOutput
    )

    @Serializable
    private data class StripOptions(
        val height: Float,
        val margin: Float,
        val minMargin: Float,
        val gap: Float,
        val minGap: Float
    )

    @Serializable
    private data class StripCoin(
        val size: Float,
        val width: Float,
        val thickness: Float,
        val partition: String
    )

    @Serializable
    private data class StripOutput(
        val turn: Float,
        val margin: Float,
        val gap: Float,
        val thinning: Float,
        val used: Float,
        val coins: List<StripPlacement>,
        val partitions: List<StripSpan>
    )

    @Serializable
    private data class StripPlacement(
        val x: Float,
        val y: Float,
        val height: Float,
        val turn: Float,
        val thick: Float,
        val visible: Float
    )

    @Serializable
    private data class StripSpan(val partition: String, val from: Float, val to: Float, val count: Int)

    @Serializable
    private data class GridVectors(val cases: List<GridCase>)

    @Serializable
    private data class GridCase(
        val label: String,
        val area: GridArea,
        val options: GridOptions,
        val input: List<GridItem>,
        val output: GridOutput
    )

    @Serializable
    private data class GridArea(val width: Float, val height: Float)

    @Serializable
    private data class GridOptions(
        val maxDiameter: Float,
        val minDiameter: Float,
        val gap: Float,
        val header: Float,
        val blockGap: Float
    )

    @Serializable
    private data class GridItem(val id: Int, val exponent: Int, val partition: String)

    @Serializable
    private data class GridOutput(
        val diameter: Float,
        val pitch: Float,
        val cols: Int,
        val cells: List<GridCell>,
        val blocks: List<GridBlock>,
        val height: Float
    )

    @Serializable
    private data class GridCell(val ids: List<Int>, val x: Float, val y: Float)

    @Serializable
    private data class GridBlock(val partition: String, val y: Float, val count: Int)

    @Serializable
    private data class MotionVectors(val spring: List<SpringSample>, val edgeCalm: List<CalmSample>)

    @Serializable
    private data class SpringSample(val input: List<Float>, val output: List<Float>)

    @Serializable
    private data class CalmSample(val turn: Float, val calm: Float)

    @Serializable
    private data class FungibilityVectors(
        val ringCapacity: Int,
        val maxLevel: Int,
        val samples: List<FungibilitySample>
    )

    @Serializable
    private data class FungibilitySample(val k: Int, val level: Int, val wear: Float)
}
