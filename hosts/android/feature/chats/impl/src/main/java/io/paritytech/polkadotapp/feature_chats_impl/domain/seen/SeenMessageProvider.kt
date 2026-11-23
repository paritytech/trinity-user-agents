package io.paritytech.polkadotapp.feature_chats_impl.domain.seen

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.common.utils.throttleLatest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.model.createSeenMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatSeenRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.LastSeenMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.isActiveChat
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChangedBy
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.mapNotNull
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

private val SEND_WINDOW = 1.seconds

private val DISPLAYABLE_CONTENT_TYPES = listOf(
    ChatMessage.Content.Text::class,
    ChatMessage.Content.RichText::class,
    ChatMessage.Content.CoinagePayment::class,
    ChatMessage.Content.DataChannelOffer::class,
    ChatMessage.Content.DataChannelAnswer::class,
    ChatMessage.Content.DataChannelClosed::class,
    ChatMessage.Content.ChatRequest::class,
    ChatMessage.Content.Unsupported::class,
)

class SeenMessageProvider @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val chatSeenRepository: ChatSeenRepository,
) {
    fun subscribeMessagesToSend(contactAccountId: AccountId): Flow<EncodedMessage> {
        val chatId = ChatId.fromContact(contactAccountId)

        return chatSeenRepository.subscribeLastSeenIncomingMessage(chatId, DISPLAYABLE_CONTENT_TYPES)
            .filterNotNull()
            .distinctUntilChangedBy { it.id }
            .throttleLatest(SEND_WINDOW)
            .mapNotNull { message -> createIfNotSent(contactAccountId, message) }
    }

    private suspend fun createIfNotSent(contactAccountId: AccountId, message: LastSeenMessage): EncodedMessage? {
        return runCancellableCatching { shouldSend(contactAccountId, message) }
            .flatMap { shouldSend -> if (shouldSend) message.createSeenMessage() else Result.success(null) }
            .logFailure("Failed to create a seen message for $contactAccountId")
            .getOrNull()
    }

    private suspend fun shouldSend(contactAccountId: AccountId, message: LastSeenMessage): Boolean {
        val contact = contactsRepository.getContact(contactAccountId) ?: return false
        if (!contact.isActiveChat()) return false

        val lastSentSeen = chatSeenRepository.getLastSentSeen(ChatId.fromContact(contactAccountId))
        return message.isNewerThan(lastSentSeen)
    }

    private fun LastSeenMessage.isNewerThan(lastSentSeen: LastSeenMessage?): Boolean {
        if (lastSentSeen == null) return true

        return sortOrder > lastSentSeen.sortOrder || (sortOrder == lastSentSeen.sortOrder && timestamp > lastSentSeen.timestamp)
    }
}
