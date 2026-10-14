package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

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
}
