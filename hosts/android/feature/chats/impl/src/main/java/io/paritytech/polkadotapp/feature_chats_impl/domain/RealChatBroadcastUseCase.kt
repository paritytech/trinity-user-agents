package io.paritytech.polkadotapp.feature_chats_impl.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.ChatBroadcastUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMessageSender
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import timber.log.Timber
import javax.inject.Inject

class RealChatBroadcastUseCase @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val chatMessageSender: ChatMessageSender,
) : ChatBroadcastUseCase {
    override suspend fun broadcastToContacts(content: ChatMessage.Content, contactFilter: (Contact) -> Boolean): Result<Unit> = runCatching {
        contactsRepository.getContacts()
            .filter { contactFilter(it) }
            .forEach { contact ->
                runCatching {
                    chatMessageSender.sendUserMessage(
                        chatId = ChatId.fromContact(contact.accountId),
                        content = content,
                    )
                }.onFailure {
                    Timber.w(it, "Failed to broadcast message to ${contact.accountId}")
                }
            }
    }
}
