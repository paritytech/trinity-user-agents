package io.paritytech.polkadotapp.feature_products_impl.data.storage

import com.google.gson.Gson
import com.google.gson.annotations.SerializedName
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductRoomFooter
import uniffi.truapi.ProductId
import javax.inject.Inject

private const val PREFS_KEY_PREFIX = "ProductRoomFooters"

/**
 * The footer each room of a product shows, kept across restarts because a room keeps its footer until the
 * product sets another.
 *
 * A room missing from the map shows the text input.
 */
interface ProductRoomFooterStorage {
    fun read(productId: ProductId): Map<ProductChatIdParameter, ProductRoomFooter>

    fun write(productId: ProductId, footers: Map<ProductChatIdParameter, ProductRoomFooter>)
}

class RealProductRoomFooterStorage @Inject constructor(
    private val preferences: Preferences,
    private val gson: Gson,
) : ProductRoomFooterStorage {
    override fun read(productId: ProductId): Map<ProductChatIdParameter, ProductRoomFooter> {
        val stored = preferences.getString(prefsKey(productId)) ?: return emptyMap()

        return runCatching {
            gson.fromJson(stored, Array<StoredFooter>::class.java).associate { ProductChatIdParameter(it.roomId) to it.toDomain() }
        }
            .logFailure("Failed to read the room footers of ${productId.value}")
            .getOrDefault(emptyMap())
    }

    override fun write(productId: ProductId, footers: Map<ProductChatIdParameter, ProductRoomFooter>) {
        val stored = footers.mapNotNull { (room, footer) -> footer.toStored(room) }
        preferences.putString(prefsKey(productId), gson.toJson(stored))
    }

    private fun prefsKey(productId: ProductId) = "$PREFS_KEY_PREFIX.${productId.value}"
}

private class StoredFooter(
    val roomId: String,
    val kind: StoredKind,
    val messageType: String? = null,
    val payloadHex: String? = null,
)

private enum class StoredKind {
    @SerializedName("Empty")
    EMPTY,

    @SerializedName("Card")
    CARD,
}

private fun ProductRoomFooter.toStored(room: ProductChatIdParameter): StoredFooter? = when (this) {
    ProductRoomFooter.TextInput -> null
    ProductRoomFooter.Empty -> StoredFooter(room.value, StoredKind.EMPTY)
    is ProductRoomFooter.Card -> StoredFooter(room.value, StoredKind.CARD, messageType, payload.toHexString())
}

private fun StoredFooter.toDomain(): ProductRoomFooter = when (kind) {
    StoredKind.EMPTY -> ProductRoomFooter.Empty
    StoredKind.CARD -> ProductRoomFooter.Card(
        messageType = requireNotNull(messageType) { "room footer card held without its message type" },
        payload = requireNotNull(payloadHex) { "room footer card held without its payload" }.fromHex(),
    )
}
