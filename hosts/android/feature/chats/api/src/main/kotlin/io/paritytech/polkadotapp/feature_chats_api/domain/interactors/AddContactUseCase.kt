package io.paritytech.polkadotapp.feature_chats_api.domain.interactors

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactOrigin
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import kotlinx.coroutines.flow.Flow
import kotlin.time.Instant

/**
 * Use-case for adding contacts to the app. Addition of a contact will initiate the relevant contact-chat subscriptions,
 * display contact chat in the chat list. It will also automatically send initial "ContactAdded" message to the peer
 */
interface AddContactUseCase {
    /**
     * Adds a contact with the specified account ID. Also sends a chat request to the specified contact
     * Contact is considered "pending" until the receiver accepts the chat request
     *
     * @param contactAccountId The account ID of the contact to add
     * @param username The optional username of the contact
     * @param chatKey The chat key for establishing encrypted communication
     * @param sharedSecretDerivationDomain domain that should be used for shared encryption key derivation.
     * Note that it should be different for different "user identities" - using same one might leak connection
     * between user identities. For example, we need to use different domain for the lite-username chat and game chat contacts
     * @param ourMetaAccountId account that will be used as an origin for chat message in statement-store
     * @param welcomeMessage - message that should be send alongside the chat request
     */
    suspend fun addContactWithChatRequest(
        contactAccountId: AccountId,
        username: Username?,
        avatar: String?,
        chatKey: ByteArray,
        sharedSecretDerivationDomain: SharedSecretDerivationDomain,
        ourMetaAccountId: Long,
        origin: ContactOrigin,
        welcomeMessage: ChatMessage.Content.RichText?
    ): Result<Unit>

    suspend fun addAlreadyEstablishedContactsById(accountIds: List<AccountId>): Result<Unit>

    suspend fun getContactsAddedAfter(after: Instant): List<Contact>

    suspend fun getEstablishedContactsAddedAfter(after: Instant): List<Contact>

    /**
     * Subscribes to the set of all contact account IDs.
     * Emits whenever contacts are added or removed.
     */
    fun subscribeContactAccountIds(): Flow<Set<AccountId>>

    fun observeContactsChanged(): Flow<Unit>
}
