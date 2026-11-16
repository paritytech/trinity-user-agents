package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.overlay

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * One message an overlay page posts through `window.ProductOverlay.postMessage(JSON.stringify(msg))`.
 *
 * The only one is `{kind:"overlayDone"}`: the page has finished and the host takes the overlay away.
 */
sealed interface ProductOverlayMessage {
    data object Done : ProductOverlayMessage

    companion object {
        const val BRIDGE_NAME = "ProductOverlay"

        /** Reads one posted message. Anything unreadable or unknown is dropped. */
        fun parse(text: String): ProductOverlayMessage? {
            val fields = runCatching { Json.parseToJsonElement(text) }.getOrNull() as? JsonObject ?: return null
            val kind = (fields["kind"] as? JsonPrimitive)?.takeIf { it.isString }?.content
            return if (kind == "overlayDone") Done else null
        }
    }
}
