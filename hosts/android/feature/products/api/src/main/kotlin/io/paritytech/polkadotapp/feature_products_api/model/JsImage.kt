package io.paritytech.polkadotapp.feature_products_api.model

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** What an image draws: bytes the tree carries, or a file the host looks up through a [JsImageSource]. */
@Serializable
sealed interface JsImageContent {
    /** Bytes the tree carries itself, which the core has allowed. */
    @Serializable
    @SerialName("inline")
    data class Inline(val bytes: DataByteArray) : JsImageContent {
        // Logged when a picture fails to draw, so it names the size rather than printing every byte.
        override fun toString() = "Inline(${bytes.value.size} bytes)"
    }
}

/** Where the host fetches an image's bytes from; the tree carries no URL. */
@Serializable
sealed interface JsImageSource : JsImageContent {
    @Serializable
    @SerialName("bulletin")
    data class Bulletin(val cid: String) : JsImageSource

    /** A file inside the product's executable archive, relative to the archive root. */
    @Serializable
    @SerialName("archive")
    data class Archive(val path: String) : JsImageSource
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
