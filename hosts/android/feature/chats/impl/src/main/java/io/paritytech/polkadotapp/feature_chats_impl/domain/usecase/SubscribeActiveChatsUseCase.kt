package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.combineToPair
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatVariant
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactWithChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.hasDeclinedIncomingRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.hasPendingIncomingRequest
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatDisplay.ChatDisplayGenerator
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummary
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class SubscribeActiveChatsUseCase @Inject constructor(
    private val chatEngine: ChatEngine,
    private val contactsRepository: ContactsRepository,
    private val chatExtensionRegistry: ChatExtensionRegistry,
    private val chatDisplayGenerator: ChatDisplayGenerator,
) {
    context(scope: ComputationalScope)
    operator fun invoke(): Flow<List<Chat>> = combineToPair(
        chatExtensionRegistry.observeActiveExtensions(),
        contactsRepository.subscribeContactsWithChatRequests(),
    ).flatMapLatest { (activeExtensions, contacts) ->
        val context = buildActiveChatsContext(activeExtensions, contacts)

        chatEngine.subscribeChatSummaries()
            .map { summaries ->
                summaries.mapNotNull { summary ->
                    val chatId = summary.chatId
                    if (!context.isChatActive(chatId)) return@mapNotNull null

                    val display = constructChatDisplay(chatId, summary, context.activeContactsById) ?: return@mapNotNull null

                    Chat(
                        id = chatId,
                        display = display,
                        preview = summary.preview,
                        timestamp = summary.timestamp,
                        unreadBadge = summary.badge,
                        hasUnseenReaction = summary.hasUnseenReaction,
                        customPreviewRenderer = summary.customPreviewRenderer
                    )
                }
            }
    }.inBackground()

    private fun buildActiveChatsContext(
        activeExtensions: List<ChatExtension>,
        contacts: List<ContactWithChatRequest>
    ): ActiveChatsContext {
        val activeExtensionIds = activeExtensions.map { it.id }

        val contactByAccountIds = contacts
            .filter {
                !it.hasPendingIncomingRequest() &&
                    !it.hasDeclinedIncomingRequest() &&
                    !it.contact.isBlocked
            }
            .associateBy { it.contact.accountId }

        return ActiveChatsContext(
            activeExtensions = activeExtensionIds,
            activeContactsById = contactByAccountIds
        )
    }

    private suspend fun constructChatDisplay(
        chatId: ChatId,
        summary: ChatSummary,
        contactsById: Map<AccountId, ContactWithChatRequest>,
    ): ChatDisplay? {
        val defaultDisplay = constructDefaultChatDisplay(chatId, contactsById) ?: return null

        return chatDisplayGenerator.applyRoomMetadata(summary.roomMetadata, defaultDisplay)
    }

    private suspend fun constructDefaultChatDisplay(
        chatId: ChatId,
        contactsById: Map<AccountId, ContactWithChatRequest>,
    ): ChatDisplay? {
        return when (val chatVariant = chatId.chatVariant()) {
            is ChatVariant.Extension -> chatEngine.getDefaultChatExtensionDisplay(chatId)
                .logFailure("Failed to load chat summary")
                .getOrNull()

            is ChatVariant.Contact -> {
                val contact = contactsById[chatVariant.contactAccountId] ?: return null
                chatEngine.getContactChatDisplay(contact.contact)
            }
        }
    }

    private data class ActiveChatsContext(
        val activeExtensions: List<ChatExtensionId>,
        val activeContactsById: Map<AccountId, ContactWithChatRequest>
    ) {
        fun isChatActive(chatId: ChatId): Boolean {
            return when (val variant = chatId.chatVariant()) {
                is ChatVariant.Contact -> variant.contactAccountId in activeContactsById
                is ChatVariant.Extension -> variant.extensionId in activeExtensions
            }
        }
    }
}
