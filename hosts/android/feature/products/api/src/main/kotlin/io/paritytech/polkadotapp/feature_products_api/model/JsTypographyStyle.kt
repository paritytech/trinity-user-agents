package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
enum class JsTypographyStyle {
    @SerialName("headline.large")
    HEADLINE_LARGE,

    @SerialName("title.medium.regular")
    TITLE_MEDIUM_REGULAR,

    @SerialName("body.large.regular")
    BODY_LARGE_REGULAR,

    @SerialName("body.medium.regular")
    BODY_MEDIUM_REGULAR,

    @SerialName("body.small.regular")
    BODY_SMALL_REGULAR,
}
