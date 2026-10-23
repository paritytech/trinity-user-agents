package io.paritytech.polkadotapp.feature_settings_impl.domain.interactors

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatActions
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContacts
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import timber.log.Timber
import javax.inject.Inject

/** A blocked user: a blocked contact of the chat product. */
data class BlockedUser(val accountId: AccountId, val username: String?, val avatarUrl: String?)

class BlockedUsersInteractor @Inject constructor(
    private val personChatContacts: PersonChatContacts,
    private val personChatActions: PersonChatActions,
) {
    // A contact that the user unblocked leaves the list at once, as in the native chat, before the product says so.
    private val unblocked = MutableStateFlow(emptySet<AccountId>())

    fun subscribeBlockedContacts(): Flow<List<BlockedUser>> =
        combine(personChatContacts.observeContacts(), unblocked) { contacts, unblocked ->
            contacts.filter { it.blocked && it.accountId !in unblocked }.map { BlockedUser(it.accountId, it.username, avatarUrl = null) }
        }

    suspend fun unblockContact(accountId: AccountId) {
        val room = productRoomOf(accountId) ?: return Timber.w("The chat product has no room with the blocked contact")

        personChatActions.unblock(room)
            .onSuccess { unblocked.update { it + accountId } }
            .onFailure { Timber.w(it, "The chat product did not unblock the contact") }
    }

    /** The room of the chat product with the contact, or null when the product has no such contact. */
    suspend fun productRoomOf(accountId: AccountId): ChatId? =
        personChatContacts.getContacts().find { it.accountId == accountId }?.chatId
}
