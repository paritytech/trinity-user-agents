package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import kotlin.math.roundToInt

/**
 * Turns the strip layout into the instances the renderer draws, applying our own banding rather than the
 * reference's.
 *
 * Geometry only. Which coins exist, which bucket each is in and what order they come in are decided before
 * they get here: both layouts start a new block wherever the partition changes, so they take display order
 * as given rather than sorting it themselves.
 *
 * The reference assigns a metal and an outline per denomination; we band four denominations to a metal and
 * run the same four outlines through each band, so its `designs.json` is read only for what belongs to the
 * geometry itself: thickness, face width, relief tile.
 */
object CoinageScene {
    /** The meshes the reference exports, by what they are. */
    enum class Geometry(val key: String) {
        ROUND("g0"),
        FLOWER("g1"),
        CURVED_POLYGON("g2"),
        ROUND_BIMETAL("g3"),
        CURVED_POLYGON_BIMETAL("g5"),
        SCALLOPED("g6"),
        FLOWER_BIMETAL("g7");

        /**
         * Face-on extent in coin heights. A property of the outline, not of the value, so it travels with
         * the mesh rather than with the denomination.
         */
        val faceWidth: Float
            get() = when (this) {
                FLOWER, FLOWER_BIMETAL -> 1.012238f
                SCALLOPED -> 0.96574f
                else -> 1f
            }
    }

    /**
     * Indices into `metals.json`, which the shader reads directly. The reference bands its own denominations
     * differently and uses all six rows; ours reaches only these three, plus silver as the twin band's core.
     */
    enum class Metal(val index: Float) {
        SILVER(3f),
        GOLD(4f),
        BRONZE(5f)
    }

    /**
     * Our four bands: dull bronze, bright silver, brighter gold, and bimetallic at the top.
     *
     * Every band runs the same four outlines in the same order, so a coin's shape names its place within its
     * metal wherever it sits. The top band has three denominations and so gets the first three. The
     * reference's own table never pairs a flower with a core, so that mesh is exported for us specifically.
     */
    fun geometry(exponent: Int): Geometry {
        val clamped = exponent.coerceIn(0, CoinageCoinDesign.HIGHEST_EXPONENT)
        val band = clamped / CoinageCoinDesign.DENOMINATIONS_PER_BAND

        if (band >= CoinageCoinDesign.Band.TWIN.ordinal) {
            val within = clamped - CoinageCoinDesign.Band.TWIN.ordinal * CoinageCoinDesign.DENOMINATIONS_PER_BAND

            return TWIN_SHAPES[minOf(within, TWIN_SHAPES.lastIndex)]
        }

        return SHAPES[clamped % CoinageCoinDesign.DENOMINATIONS_PER_BAND]
    }

    fun metals(exponent: Int): Pair<Metal, Metal?> = when (CoinageCoinDesign.band(exponent)) {
        CoinageCoinDesign.Band.BRONZE -> Metal.BRONZE to null
        CoinageCoinDesign.Band.SILVER -> Metal.SILVER to null
        CoinageCoinDesign.Band.GOLD -> Metal.GOLD to null
        CoinageCoinDesign.Band.TWIN -> Metal.GOLD to Metal.SILVER
    }

    /** One holding, as much of it as the renderer and the packing need. */
    data class Coin(
        val id: String,
        val exponent: Int,
        /** `0` untraceable, `1` fully traceable. */
        val wear: Float,
        val partition: CoinageStripLayout.Partition,
        /**
         * How hidden the coin is, on the doubling ladder, which is how the order breaks a tie between two
         * coins of the same denomination.
         */
        val level: Int,
        /** Payments this holding has been through. Each one leaves a pit. */
        val hops: Int
    )

    /**
     * Reads the field's live spring values rather than the layout's targets, so what is drawn is wherever
     * each coin has actually got to.
     */
    fun batches(field: CoinageCoinField, designs: List<CoinageAssetStore.Design>): List<CoinageBatch> {
        val batches = LinkedHashMap<String, MutableList<CoinageInstance>>()

        for (member in field.members) {
            val exponent = member.coin.exponent.coerceIn(0, CoinageCoinDesign.HIGHEST_EXPONENT)
            val design = designs.getOrNull(exponent) ?: continue

            batches.getOrPut(geometry(member.coin.exponent).key) { mutableListOf() } +=
                instance(member, design, field.distanceToTarget(member))
        }

        return batches.map { (geometry, instances) -> CoinageBatch(geometry, instances) }
    }

    private fun instance(
        member: CoinageCoinField.Member,
        design: CoinageAssetStore.Design,
        distance: Float
    ): CoinageInstance {
        val (outer, core) = metals(member.coin.exponent)
        val channels = member.channels
        // Luster is the six-tap path, so it fades in only as a coin lands.
        val landing = maxOf(0f, 1f - distance / 40f)

        return CoinageInstance(
            // World y runs up, so screen y is negated here and never in the shader.
            positionX = channels.centreX.value,
            positionY = -channels.centreY.value,
            positionZ = channels.lift.value,
            height = channels.height.value,
            // The strip turns coins by a negative angle; spin about z is for the detail view.
            turn = -channels.turn.value,
            tilt = channels.tilt.value,
            spin = 0f,
            thickness = channels.thickness.value,
            wear = channels.wear.value,
            outerMetal = outer.index,
            coreMetal = core?.index ?: -1f,
            tile = design.tile,
            reeds = reeds(member.coin.exponent),
            luster = channels.luster.value * landing * landing,
            recede = 0f,
            calm = channels.calm.value,
            // Streaks follow the animated wear, so a coin visibly cleans up as its ring fills. Pits do not:
            // a payment happened or it did not.
            pits = member.coin.hops.toFloat(),
            streaks = channels.wear.value,
            seed = member.seed
        )
    }

    /**
     * Milling is reserved for the round coins above bronze, as a mint reserves it for the coins worth
     * protecting. The count is what the rim can actually resolve.
     */
    private fun reeds(exponent: Int): Float {
        val design = CoinageCoinDesign.design(exponent)

        if (!design.isReeded) return 0f

        return (60f + 70f * design.size).roundToInt().toFloat()
    }

    private val SHAPES = listOf(
        Geometry.ROUND,
        Geometry.CURVED_POLYGON,
        Geometry.FLOWER,
        Geometry.SCALLOPED
    )

    private val TWIN_SHAPES = listOf(
        Geometry.ROUND_BIMETAL,
        Geometry.CURVED_POLYGON_BIMETAL,
        Geometry.FLOWER_BIMETAL
    )
}

/** What one coin hands the GPU: five `vec4`, interleaved, 80 bytes. */
data class CoinageInstance(
    val positionX: Float,
    val positionY: Float,
    val positionZ: Float,
    val height: Float,
    val turn: Float,
    val tilt: Float,
    val spin: Float,
    val thickness: Float,
    val wear: Float,
    val outerMetal: Float,
    val coreMetal: Float,
    val tile: Float,
    val reeds: Float,
    val luster: Float,
    val recede: Float,
    val calm: Float,
    val pits: Float,
    val streaks: Float,
    val seed: Float
)

/** A run of instances sharing a mesh, which is what an instanced draw needs. */
data class CoinageBatch(val geometry: String, val instances: List<CoinageInstance>)
