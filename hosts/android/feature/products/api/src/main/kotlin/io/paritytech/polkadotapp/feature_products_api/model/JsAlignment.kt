package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
enum class JsAlignment {
    @SerialName("topStart")
    TOP_START,

    @SerialName("topCenter")
    TOP_CENTER,

    @SerialName("topEnd")
    TOP_END,

    @SerialName("centerStart")
    CENTER_START,

    @SerialName("center")
    CENTER,

    @SerialName("centerEnd")
    CENTER_END,

    @SerialName("bottomStart")
    BOTTOM_START,

    @SerialName("bottomCenter")
    BOTTOM_CENTER,

    @SerialName("bottomEnd")
    BOTTOM_END,
}

@Serializable
enum class JsHorizontalAlignment {
    @SerialName("start")
    START,

    @SerialName("center")
    CENTER,

    @SerialName("end")
    END,
}

@Serializable
enum class JsVerticalAlignment {
    @SerialName("top")
    TOP,

    @SerialName("center")
    CENTER,

    @SerialName("bottom")
    BOTTOM,
}

@Serializable
enum class JsArrangement {
    @SerialName("start")
    START,

    @SerialName("end")
    END,

    @SerialName("center")
    CENTER,

    @SerialName("spaceBetween")
    SPACE_BETWEEN,

    @SerialName("spaceAround")
    SPACE_AROUND,

    @SerialName("spaceEvenly")
    SPACE_EVENLY,
}
