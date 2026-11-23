package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import uniffi.truapi.ImageSource

/** Where an image's bytes come from. The host fetches them; the tree carries no URL. */
@Serializable
sealed interface JsImageSource {
    @Serializable
    @SerialName("bulletin")
    data class Bulletin(val cid: String) : JsImageSource

    /** A file inside the product's executable archive, relative to the archive root. */
    @Serializable
    @SerialName("archive")
    data class Archive(val path: String) : JsImageSource
}

/** The source the image loader fetches for a core image source; inline bytes have none. */
fun ImageSource.toJsImageSource(): JsImageSource? = when (this) {
    is ImageSource.Bulletin -> JsImageSource.Bulletin(v1)
    is ImageSource.Archive -> JsImageSource.Archive(v1)
    is ImageSource.Inline -> null
}

@Serializable
enum class JsImageFit {
    NONE,
    FILL,
    COVER,
    CONTAIN,
    SCALE_DOWN,
}

@Serializable
enum class JsBlendingMode {
    NORMAL,
    MULTIPLY,
    SCREEN,
    OVERLAY,
    DARKEN,
    LIGHTEN,
    COLOR_DODGE,
    COLOR_BURN,
    HARD_LIGHT,
    SOFT_LIGHT,
    DIFFERENCE,
    EXCLUSION,
    HUE,
    SATURATION,
    COLOR,
    LUMINOSITY,
}
