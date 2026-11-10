package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

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

    /** Bytes the tree carries itself, which the core has allowed. They need no lookup. */
    @Serializable
    @SerialName("inline")
    class Inline(val bytes: ByteArray) : JsImageSource {
        // Compared by content, so a redrawn face does not load the same picture again.
        override fun equals(other: Any?) = other is Inline && bytes.contentEquals(other.bytes)

        override fun hashCode() = bytes.contentHashCode()
    }
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
enum class JsEffect {
    RAINBOW,
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
