package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** What one particle looks like. Sizes are hundredths of a dp and colours `0xRRGGBBAA`. */
@Serializable
sealed interface JsParticleLook {
    @Serializable
    @SerialName("circle")
    data class Circle(val diameter: Long, val rgba: Long) : JsParticleLook

    @Serializable
    @SerialName("rectangle")
    data class Rectangle(val width: Long, val height: Long, val rgba: Long) : JsParticleLook

    /** An image drawn at this size; with no [source] the host cannot fetch it, and the particle is not drawn. */
    @Serializable
    @SerialName("image")
    data class Image(val source: JsImageSource?, val width: Long, val height: Long) : JsParticleLook
}

/** Where particles start, in the node's frame. */
@Serializable
sealed interface JsEmitSource {
    /** Where the finger is on the location pan keyed [key], only while it is down. */
    @Serializable
    @SerialName("pan")
    data class Pan(val key: String) : JsEmitSource

    /** A place in hundredths of a dp from the node's top-start corner. */
    @Serializable
    @SerialName("point")
    data class Point(val x: Int, val y: Int) : JsEmitSource

    /** Anywhere in the node, evenly at random. */
    @Serializable
    @SerialName("area")
    data object Area : JsEmitSource
}

/** A value picked evenly at random between [min] and [max], the two swapped when [max] is less. */
@Serializable
data class JsValueRange(val min: Int, val max: Int) {
    /** The value at [share] (0 through 1) of the way from the least to the greatest. */
    fun pick(share: Double): Double {
        val low = minOf(min, max).toDouble()
        val high = maxOf(min, max).toDouble()
        return low + (high - low) * share.coerceIn(0.0, 1.0)
    }
}

/** What sets off a [JsBurst]. */
@Serializable
sealed interface JsBurstTrigger {
    /** The node appearing. */
    @Serializable
    @SerialName("appear")
    data object Appear : JsBurstTrigger

    /** The host sending [action] from the current tree, whichever node sends it. */
    @Serializable
    @SerialName("action")
    data class Action(val action: String) : JsBurstTrigger

    /** A tree carrying a different [value] here than the tree before it. */
    @Serializable
    @SerialName("change")
    data class Change(val value: Long) : JsBurstTrigger
}

@Serializable
data class JsBurst(val count: Int, val on: JsBurstTrigger)

/**
 * Particles the host sends from a node and moves on its own clock. Speeds are hundredths of a dp a second, angles
 * hundredths of a degree clockwise from the end direction, shares thousandths.
 */
@Serializable
data class JsEmitter(
    val looks: List<JsParticleLook>,
    val source: JsEmitSource,
    val ratePerS: Int,
    val ratePer100pt: Int,
    val lifeMs: Int,
    val speed: JsValueRange,
    val angleCentideg: Int,
    val spreadCentideg: Int,
    val gravity: Int,
    val spin: JsValueRange,
    val fadeFromPermille: Int,
    val endScalePermille: Int,
    val maxAlive: Int,
    val bursts: List<JsBurst> = emptyList(),
)
