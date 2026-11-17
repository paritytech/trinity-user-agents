package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.overlay

import io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web.ProductRoomFooterWebPage
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import java.net.URI
import java.net.URISyntaxException

/** Where an overlay loads from: one of the product's own pages. */
object ProductOverlayPage {
    /**
     * The overlay page at [path] on the product's own origin, or null when [path] would leave it or names no file.
     *
     * The page must be a file with an extension, such as `confetti-overlay.html`. An empty path, a bare query or
     * fragment, or a path with no extension names no page: the content client would answer it with a 404, and the iOS
     * host's content handler with the product's whole app, so both hosts refuse it before showing anything.
     */
    fun url(path: String, productId: String): String? {
        val url = ProductRoomFooterWebPage.url(path, productId) ?: return null
        val file = try {
            URI(url).path.orEmpty().substringAfterLast('/')
        } catch (_: URISyntaxException) {
            return null
        }
        return url.takeIf { file.substringAfterLast('.', missingDelimiterValue = "").isNotEmpty() }
    }
}

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
