package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import uniffi.truapi.HostChatPushRoute
import java.util.Base64
import javax.inject.Inject
import javax.inject.Singleton

/** A chat push route of a product: the room a push of [pushId] opens, its title, and the key its payload opens with. */
class ProductChatPushRoute(val productId: String, val pushId: String, val roomId: String, val title: String, val key: ByteArray)

/**
 * The chat push routes the chat product gives (`setPushRoutes`), kept encrypted: each holds the key of a contact's
 * pushes. The push handler reads them while no chat worker runs.
 */
@Singleton
class ProductChatPushRoutes @Inject constructor(private val preferences: EncryptedPreferences) {
    @Volatile
    private var routes: List<ProductChatPushRoute>? = null

    fun replace(productId: String, replacement: List<HostChatPushRoute>) = synchronized(this) {
        val kept = all().filter { it.productId != productId } +
            replacement.map { ProductChatPushRoute(productId, it.pushId.lowercase(), it.roomId, it.title, it.key) }
        preferences.putEncryptedString(FIELD, kept.joinToString(separator = "\n", transform = ::encode))
        routes = kept
    }

    fun find(pushId: String): ProductChatPushRoute? = all().firstOrNull { it.pushId == pushId.lowercase() }

    private fun all(): List<ProductChatPushRoute> = routes ?: synchronized(this) {
        routes ?: preferences.getDecryptedString(FIELD).orEmpty().lines().mapNotNull(::decode).also { routes = it }
    }

    private fun encode(route: ProductChatPushRoute): String =
        listOf(route.productId.toByteArray(), route.pushId.toByteArray(), route.roomId.toByteArray(), route.title.toByteArray(), route.key)
            .joinToString(separator = " ") { Base64.getEncoder().encodeToString(it) }

    private fun decode(line: String): ProductChatPushRoute? {
        val parts = line.split(" ").takeIf { it.size == PARTS } ?: return null
        val decoded = runCatching { parts.map { Base64.getDecoder().decode(it) } }.getOrNull() ?: return null
        return ProductChatPushRoute(
            productId = String(decoded[0]),
            pushId = String(decoded[1]),
            roomId = String(decoded[2]),
            title = String(decoded[3]),
            key = decoded[4],
        )
    }

    private companion object {
        const val FIELD = "product_chat_push_routes"
        const val PARTS = 5
    }
}
