package io.paritytech.polkadotapp.feature_settings_impl.domain.interactors

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.BlockedContactsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatActions
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContacts
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatMode
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import timber.log.Timber
import javax.inject.Inject

/** A blocked user: a blocked contact of the native chat, or of the chat product while it runs the chats. */
data class BlockedUser(val accountId: AccountId, val username: String?, val avatarUrl: String?)

class BlockedUsersInteractor @Inject constructor(
    private val repository: BlockedContactsRepository,
    private val personChatMode: PersonChatMode,
    private val personChatContacts: PersonChatContacts,
    private val personChatActions: PersonChatActions,
) {
    // A contact that the user unblocked leaves the list at once, as in the native chat, before the product says so.
    private val unblocked = MutableStateFlow(emptySet<AccountId>())

    fun subscribeBlockedContacts(): Flow<List<BlockedUser>> = if (personChatMode.isProductChat()) {
        combine(personChatContacts.observeContacts(), unblocked) { contacts, unblocked ->
            contacts.filter { it.blocked && it.accountId !in unblocked }.map { BlockedUser(it.accountId, it.username, avatarUrl = null) }
        }
    } else {
        repository.subscribeBlockedContacts().map { contacts -> contacts.map { BlockedUser(it.accountId, it.username, it.avatarUrl) } }
    }

    suspend fun unblockContact(accountId: AccountId) {
        val room = productRoomOf(accountId)
        if (room == null) return repository.setBlocked(accountId, false)

        personChatActions.unblock(room)
            .onSuccess { unblocked.update { it + accountId } }
            .onFailure { Timber.w(it, "The chat product did not unblock the contact") }
    }

    /** The room of the chat product with the contact while it runs the chats, else null. */
    suspend fun productRoomOf(accountId: AccountId): ChatId? {
        if (!personChatMode.isProductChat()) return null
        return personChatContacts.getContacts().find { it.accountId == accountId }?.chatId
    }
}
