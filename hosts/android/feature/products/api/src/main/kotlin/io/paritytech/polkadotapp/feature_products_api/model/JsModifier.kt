package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.Transient

@Serializable
sealed interface JsModifier {
    @Serializable
    @SerialName("margin")
    data class Margin(
        val all: Int? = null,
        val horizontal: Int? = null,
        val vertical: Int? = null,
        val start: Int? = null,
        val top: Int? = null,
        val end: Int? = null,
        val bottom: Int? = null,
    ) : JsModifier

    @Serializable
    @SerialName("padding")
    data class Padding(
        val all: Int? = null,
        val horizontal: Int? = null,
        val vertical: Int? = null,
        val start: Int? = null,
        val top: Int? = null,
        val end: Int? = null,
        val bottom: Int? = null,
    ) : JsModifier

    @Serializable
    @SerialName("background")
    data class Background(
        val color: JsColor,
        val shape: JsShape? = null,
    ) : JsModifier

    @Serializable
    @SerialName("border")
    data class Border(
        val width: Int,
        val color: JsColor,
        val shape: JsShape? = null,
    ) : JsModifier

    @Serializable
    @SerialName("size")
    data class Size(
        val width: Int? = null,
        val height: Int? = null,
        val minWidth: Int? = null,
        val maxWidth: Int? = null,
        val minHeight: Int? = null,
        val maxHeight: Int? = null,
    ) : JsModifier

    @Serializable
    @SerialName("fillMaxWidth")
    data class FillMaxWidth(
        val fraction: Float = 1f,
    ) : JsModifier

    @Serializable
    @SerialName("fillMaxHeight")
    data class FillMaxHeight(
        val fraction: Float = 1f,
    ) : JsModifier

    @Serializable
    @SerialName("clip")
    data class Clip(
        val shape: JsShape,
    ) : JsModifier

    /** 0 is transparent, 255 is opaque. */
    @Serializable
    @SerialName("opacity")
    data class Opacity(
        val alpha: Int,
    ) : JsModifier

    @Serializable
    @SerialName("blendingMode")
    data class BlendingMode(
        val mode: JsBlendingMode,
    ) : JsModifier

    /** A colour by value (0xRRGGBBAA) in an optional shape; it draws as a background does and clips nothing. */
    @Serializable
    @SerialName("fill")
    data class Fill(
        val rgba: Long,
        val shape: JsShape? = null,
    ) : JsModifier

    /** CSS `box-shadow`: the node's fill shape, moved by [x], [y] and blurred by [blur] (dp), under the node. */
    @Serializable
    @SerialName("shadow")
    data class Shadow(
        val rgba: Long,
        val x: Float,
        val y: Float,
        val blur: Float,
    ) : JsModifier

    /** Draws the node moved by [x], [y] (dp) without moving its layout. */
    @Serializable
    @SerialName("translate")
    data class Translate(
        val x: Float,
        val y: Float,
    ) : JsModifier

    /** A text's type by value: size and line height in dp, CSS weight, colour, letter spacing in dp. */
    @Serializable
    @SerialName("font")
    data class Font(
        val size: Float,
        val lineHeight: Float,
        val weight: Int,
        val rgba: Long,
        val letterSpacing: Float,
    ) : JsModifier

    /** Opacity `1 - rate × progress` of the keyed gesture's children along their travel. */
    @Serializable
    @SerialName("fadeWithPan")
    data class FadeWithPan(
        val key: String,
        val rate: Float,
    ) : JsModifier

    /**
     * A band of [rgba] light over the node's drawn pixels only, its alpha from [stops] spread evenly across [band] dp,
     * moving at [speed] dp/s from [lead] before the node to [lead] past it, then resting [restMs].
     */
    @Serializable
    @SerialName("shimmer")
    data class Shimmer(
        val rgba: Long,
        val stops: List<Float>,
        val band: Float,
        val lead: Float,
        val speed: Float,
        val restMs: Int,
    ) : JsModifier

    /** One turn every [periodMs] about the centre. */
    @Serializable
    @SerialName("spin")
    data class Spin(
        val periodMs: Int,
    ) : JsModifier

    /** When a tree drops the node, it stays as it last drew and fades out over [durationMs] with CSS's `ease`. */
    @Serializable
    @SerialName("exitFade")
    data class ExitFade(
        val durationMs: Int,
    ) : JsModifier

    /**
     * Hides part of the node, or shows only part of it, with shapes in hundredths of a dp from its top-start corner.
     * @param key Names the mask for shapes added between trees and for a fade between trees
     * @param follow The location pan whose strokes the host draws into the mask itself
     * @param fadeMs How long a new tree that changes the keyed mask cross-fades into it
     * @param clearedShare The share of the node the shapes must cover before the host sends an action
     * @param channel The shapes added since this tree, and the fade into it
     * @param trail The followed pan's strokes since this tree
     */
    @Serializable
    @SerialName("mask")
    data class Mask(
        val key: String? = null,
        val mode: JsMaskMode = JsMaskMode.HIDE,
        val shapes: List<JsMaskShape> = emptyList(),
        val follow: JsMaskFollow? = null,
        val fadeMs: Long = 0,
        val clearedShare: JsClearedShare? = null,
        val haptics: JsMaskHaptics? = null,
        @Transient val channel: JsMaskShapes? = null,
        @Transient val trail: JsPanTrail? = null,
    ) : JsModifier

    @Serializable
    @SerialName("enter")
    data class Enter(val enter: JsEnter) : JsModifier

    @Serializable
    @SerialName("animate")
    data class Animate(val track: JsTrack) : JsModifier
}
