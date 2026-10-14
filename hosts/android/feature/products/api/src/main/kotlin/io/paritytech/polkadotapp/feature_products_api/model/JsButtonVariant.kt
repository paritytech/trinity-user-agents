package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
enum class JsButtonVariant {
    @SerialName("primary")
    PRIMARY,

    @SerialName("secondary")
    SECONDARY,

    @SerialName("text")
    TEXT,
}
