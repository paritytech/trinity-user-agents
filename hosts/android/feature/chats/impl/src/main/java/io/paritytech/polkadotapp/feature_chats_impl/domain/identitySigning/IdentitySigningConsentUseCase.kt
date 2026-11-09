package io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.IdentitySigningConsentRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.ContactChatSessionRestarter
import io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.signer.ContactChatSigners
import kotlinx.coroutines.flow.Flow
import timber.log.Timber
import javax.inject.Inject

// Not sent by the chat session: the request and the acceptance fail at their own next delivery attempt, which sees the
// decline, and a token is shared again when it changes.
private val KEPT_ON_DECLINE = listOf(
    ChatMessage.Content.ChatRequest::class,
    ChatMessage.Content.DeviceChatAccepted::class,
    ChatMessage.Content.Token::class,
)

private val PENDING_STATUSES = listOf(ChatMessage.Status.NEW, ChatMessage.Status.PROCESSING)

class IdentitySigningConsentUseCase @Inject constructor(
    private val consentRepository: IdentitySigningConsentRepository,
    private val contactsRepository: ContactsRepository,
    private val chatMessageRepository: ChatMessageRepository,
    private val chatSigners: ContactChatSigners,
    private val sessionRestarter: ContactChatSessionRestarter,
) {
    fun subscribe(contactAccountId: AccountId): Flow<IdentitySigningConsent?> {
        return consentRepository.subscribe(contactAccountId)
    }

    suspend fun allow(contactAccountId: AccountId): Result<Unit> = runCancellableCatching {
        Timber.i("identitySigning: the user allowed the identity signer for $contactAccountId")
        consentRepository.set(contactAccountId, IdentitySigningConsent.GRANTED)
    }

    /**
     * chat-spec RFC-0005 §4: the messages waiting for a signature fail. The session is restarted so it drops them from
     * its queue instead of sending them once a slot frees up.
     */
    suspend fun decline(contactAccountId: AccountId): Result<Unit> = runCancellableCatching {
        Timber.i("identitySigning: the user declined the identity signer for $contactAccountId")

        contactsRepository.withTransaction {
            consentRepository.set(contactAccountId, IdentitySigningConsent.DECLINED)
            failPendingMessages(contactAccountId)
        }

        chatSigners.release(contactAccountId)
        sessionRestarter.restartSession(contactAccountId)
    }

    /** Forgets the user's answer, so the chat asks again before its next identity signature. */
    suspend fun revoke(contactAccountId: AccountId) {
        consentRepository.clear(contactAccountId)
    }

    /** A message the user sends after declining asks again, should it need the identity signer too. */
    suspend fun reopenIfDeclined(contactAccountId: AccountId): Result<Unit> = runCancellableCatching {
        if (consentRepository.get(contactAccountId) == IdentitySigningConsent.DECLINED) {
            consentRepository.clear(contactAccountId)
        }
    }

    private suspend fun failPendingMessages(contactAccountId: AccountId) {
        val chatId = ChatId.fromContact(contactAccountId)

        PENDING_STATUSES.forEach { status ->
            chatMessageRepository.updateOutgoingMessagesStatusForChatExcludingTypes(
                chatId = chatId,
                fromStatus = status,
                toStatus = ChatMessage.Status.DELIVERY_FAILED,
                contentTypes = KEPT_ON_DECLINE,
            )
        }
    }
}
