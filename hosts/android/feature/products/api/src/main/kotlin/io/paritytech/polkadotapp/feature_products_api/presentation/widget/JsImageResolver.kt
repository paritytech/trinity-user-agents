package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.runtime.compositionLocalOf
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource

/** Turns an image source into something the image loader can fetch, or null for one the host cannot. */
fun interface JsImageResolver {
    suspend fun resolve(source: JsImageSource): Any?

    /**
     * What [resolve] would answer without waiting, for a host that already holds it. A face redrawn
     * somewhere else, as an expanding card is, paints its images on its first frame rather than
     * showing an empty card until they arrive.
     */
    fun resolved(source: JsImageSource): Any? = null
}

val LocalJsImageResolver = compositionLocalOf<JsImageResolver> { JsImageResolver { null } }
