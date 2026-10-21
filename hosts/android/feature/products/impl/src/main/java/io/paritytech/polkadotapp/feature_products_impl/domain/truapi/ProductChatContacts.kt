package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContact
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContacts
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import uniffi.truapi.HostChatContact
import java.util.Base64
import javax.inject.Inject
import javax.inject.Singleton

/** The contacts the chat product gives (`publishContacts`), kept encrypted across restarts. */
@Singleton
class ProductChatContacts @Inject constructor(private val preferences: EncryptedPreferences) : PersonChatContacts {
    private class Stored(val productId: String, val contact: HostChatContact)

    private val stored = MutableStateFlow<List<Stored>?>(null)

    fun replace(productId: String, replacement: List<HostChatContact>) = synchronized(this) {
        val kept = all().filter { it.productId != productId } + replacement.map { Stored(productId, it) }
        preferences.putEncryptedString(FIELD, kept.joinToString(separator = "\n", transform = ::encode))
        stored.value = kept
    }

    override suspend fun getContacts(): List<PersonChatContact> = all().map(::toContact)

    override fun observeContacts(): Flow<List<PersonChatContact>> =
        stored.onStart { all() }.filterNotNull().map { list -> list.map(::toContact) }

    private fun all(): List<Stored> = stored.value ?: synchronized(this) {
        stored.value ?: preferences.getDecryptedString(FIELD).orEmpty().lines().mapNotNull(::decode).also { stored.value = it }
    }

    private fun toContact(stored: Stored) = PersonChatContact(
        accountId = stored.contact.accountId.toDataByteArray(),
        username = stored.contact.username,
        chatId = ProductChatIdParameter(stored.contact.roomId)
            .toChatId(ProductId.fromStoredValue(stored.productId).toChatExtensionId()),
        blocked = stored.contact.blocked,
    )

    private fun encode(stored: Stored): String = listOf(
        stored.productId.toByteArray(),
        stored.contact.accountId,
        stored.contact.roomId.toByteArray(),
        byteArrayOf(if (stored.contact.blocked) 1 else 0),
        stored.contact.username?.toByteArray(),
    ).joinToString(separator = " ") { part -> part?.let { Base64.getEncoder().encodeToString(it) } ?: NO_USERNAME }

    private fun decode(line: String): Stored? {
        val parts = line.split(" ").takeIf { it.size == PARTS } ?: return null
        val username = parts.last().takeIf { it != NO_USERNAME }
        val decoded = runCatching { (parts.dropLast(1) + listOfNotNull(username)).map { Base64.getDecoder().decode(it) } }
            .getOrNull() ?: return null
        return Stored(
            productId = String(decoded[0]),
            contact = HostChatContact(
                accountId = decoded[1],
                username = decoded.getOrNull(4)?.let(::String),
                roomId = String(decoded[2]),
                blocked = decoded[3].firstOrNull() == 1.toByte(),
            ),
        )
    }

    private companion object {
        const val FIELD = "product_chat_contacts"
        const val PARTS = 5

        // Not Base64, so a missing username and an empty one stay apart.
        const val NO_USERNAME = "-"
    }
}
