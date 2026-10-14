package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
sealed interface JsShape {
    @Serializable
    @SerialName("rounded")
    data class Rounded(
        val radius: Int = 0,
        val topStart: Int? = null,
        val topEnd: Int? = null,
        val bottomStart: Int? = null,
        val bottomEnd: Int? = null,
    ) : JsShape

    @Serializable
    @SerialName("circle")
    data object Circle : JsShape

    @Serializable
    @SerialName("square")
    data object Square : JsShape
}
