package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
enum class JsColor {
    @SerialName("bg.surface.main")
    BG_SURFACE_MAIN,

    @SerialName("bg.surface.container")
    BG_SURFACE_CONTAINER,

    @SerialName("bg.surface.nested")
    BG_SURFACE_NESTED,

    @SerialName("fg.primary")
    FG_PRIMARY,

    @SerialName("fg.secondary")
    FG_SECONDARY,

    @SerialName("fg.tertiary")
    FG_TERTIARY,

    @SerialName("fg.error")
    FG_ERROR,

    @SerialName("fg.success")
    FG_SUCCESS,

    @SerialName("fg.warning")
    FG_WARNING,

    @SerialName("fg.staticWhite")
    FG_STATIC_WHITE,
}
