package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
enum class JsSlideToConfirmVariant {
    @SerialName("thumb")
    THUMB,

    @SerialName("reveal")
    REVEAL,
}
