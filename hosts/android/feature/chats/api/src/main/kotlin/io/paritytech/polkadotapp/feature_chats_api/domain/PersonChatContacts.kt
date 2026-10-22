package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import kotlinx.coroutines.flow.Flow

/**
 * The contacts of the person chat product, which it gives the host (`PersonChat.publishContacts`) for the parts of
 * the app outside the chat. Blocked contacts are included and marked.
 */
interface PersonChatContacts {
    suspend fun getContacts(): List<PersonChatContact>

    /** The same list, emitted again whenever it changes. */
    fun observeContacts(): Flow<List<PersonChatContact>>
}

/**
 * A username of the chat product as the native chat shows a contact's username: the index with two digits, as
 * `Username.fromFullValue` reads a full username (`alice.7` shows as `alice.07`). A name without an index stays.
 */
fun personChatDisplayUsername(username: String): String {
    val index = username.substringAfterLast(Username.SEPARATOR, missingDelimiterValue = "")
    if (index.isEmpty() || !index.all(Char::isDigit)) return username
    return Username.fromFullValue(username).getDisplayUsername()
}

/** A contact of the person chat product, and the room of its chat. */
data class PersonChatContact(
    val accountId: AccountId,
    val username: String?,
    val chatId: ChatId,
    val blocked: Boolean,
)
