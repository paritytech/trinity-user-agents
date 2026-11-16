package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.doubleOrNull
import java.net.URI
import java.net.URISyntaxException

/**
 * Where a web room footer loads from, and how tall it may be.
 *
 * The page is the product's own: its path is joined onto `https://<product>/`, the origin the host serves the product's
 * pages from, and a result that lands anywhere else is refused.
 */
object ProductRoomFooterWebPage {
    const val MIN_HEIGHT_DP = 44
    const val MAX_HEIGHT_DP = 160

    private const val SCHEME = "https"
    private const val ABOUT_BLANK = "about:blank"

    fun height(requested: Int): Int = requested.coerceIn(MIN_HEIGHT_DP, MAX_HEIGHT_DP)

    fun height(reported: Double?): Int? {
        if (reported == null || !reported.isFinite() || reported <= 0) return null
        return kotlin.math.ceil(reported).coerceIn(MIN_HEIGHT_DP.toDouble(), MAX_HEIGHT_DP.toDouble()).toInt()
    }

    fun origin(productId: String): String = "$SCHEME://${productId.lowercase()}"

    /** The page at [path] on the product's own origin, or null when [path] would leave it. */
    fun url(path: String, productId: String): String? {
        val trimmed = path.trim()
        if (trimmed.isEmpty() || trimmed.startsWith("/") || trimmed.contains('\\') || hasParentSegment(trimmed)) return null
        val resolved = try {
            URI("${origin(productId)}/").resolve(URI(trimmed))
        } catch (_: URISyntaxException) {
            return null
        } catch (_: IllegalArgumentException) {
            return null
        }
        return resolved.toString().takeIf { isProductOrigin(resolved, productId) }
    }

    fun isProductOrigin(url: String, productId: String): Boolean {
        val uri = try {
            URI(url)
        } catch (_: URISyntaxException) {
            return false
        }
        return isProductOrigin(uri, productId)
    }

    /** Navigation stays on the product's origin; `about:blank` is the only other document allowed. */
    fun isAllowedNavigation(url: String, productId: String): Boolean = url == ABOUT_BLANK || isProductOrigin(url, productId)

    private fun isProductOrigin(uri: URI, productId: String): Boolean =
        uri.scheme.equals(SCHEME, ignoreCase = true) &&
            uri.host.equals(productId, ignoreCase = true) &&
            uri.rawUserInfo == null &&
            uri.port == -1

    private fun hasParentSegment(path: String): Boolean =
        path.substringBefore('?').substringBefore('#').split('/').any { it == ".." || it.equals("%2e%2e", ignoreCase = true) }
}

/** One message the footer page posts through `window.ProductFooter.postMessage(JSON.stringify(msg))`. */
sealed interface ProductFooterMessage {
    data class Action(val actionId: String, val payload: String?) : ProductFooterMessage

    data class Haptic(val style: String) : ProductFooterMessage

    data class Ready(val height: Double?) : ProductFooterMessage

    companion object {
        const val BRIDGE_NAME = "ProductFooter"
        const val ACTION_ID_MAX_BYTES = 256
        const val PAYLOAD_MAX_BYTES = 64 * 1024

        private const val DEFAULT_HAPTIC = "success"

        /** Reads one posted message. Anything unreadable, unknown or oversized is dropped. */
        fun parse(text: String): ProductFooterMessage? {
            val fields = runCatching { Json.parseToJsonElement(text) }.getOrNull() as? JsonObject ?: return null
            return when (fields.string("kind")) {
                "action" -> action(fields)
                "haptic" -> Haptic(fields.string("style") ?: DEFAULT_HAPTIC)
                "ready" -> Ready((fields["height"] as? JsonPrimitive)?.takeIf { !it.isString }?.doubleOrNull)
                else -> null
            }
        }

        private fun action(fields: JsonObject): Action? {
            val actionId = fields.string("actionId")
            if (actionId.isNullOrEmpty() || actionId.utf8Size() > ACTION_ID_MAX_BYTES) return null
            val payload = payload(fields["payload"])
            if (payload != null && payload.utf8Size() > PAYLOAD_MAX_BYTES) return null
            return Action(actionId, payload)
        }

        /** A text payload travels as it is, any other JSON value as its JSON text. */
        private fun payload(raw: JsonElement?): String? = when {
            raw == null || raw is JsonNull -> null
            raw is JsonPrimitive && raw.isString -> raw.content.ifEmpty { null }
            else -> raw.toString()
        }

        private fun JsonObject.string(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

        private fun String.utf8Size(): Int = toByteArray(Charsets.UTF_8).size
    }
}

/** What the host tells the page through `window.productFooter.receive`. */
sealed interface ProductFooterHostEvent {
    data object Reset : ProductFooterHostEvent

    data object Confirmed : ProductFooterHostEvent

    val json: String
        get() = when (this) {
            Reset -> """{"kind":"reset"}"""
            Confirmed -> """{"kind":"state","state":"confirmed"}"""
        }

    /** Guarded, so a page that defines no receiver works without one. */
    val script: String
        get() = "window.productFooter && typeof window.productFooter.receive === 'function' && window.productFooter.receive($json);"
}

/** One confirm per slide: a page that posts twice in a burst sends once. */
class ProductFooterActionDebounce(
    private val windowMillis: Long,
    private val nowMillis: () -> Long,
) {
    private var lastAcceptedAt: Long? = null

    fun tryAccept(): Boolean {
        val now = nowMillis()
        val last = lastAcceptedAt
        if (last != null && now - last < windowMillis) return false
        lastAcceptedAt = now
        return true
    }
}
