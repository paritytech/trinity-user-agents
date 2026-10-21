package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
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

/** A contact of the person chat product, and the room of its chat. */
data class PersonChatContact(
    val accountId: AccountId,
    val username: String?,
    val chatId: ChatId,
    val blocked: Boolean,
)
