package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReaction
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedRejection
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageIds
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.CustomContentDecoder
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.MessageRevisionRepository
import javax.inject.Inject

/**
 * Stores the feed updates that an extension pushes to a room whose messages the host keeps (Chat Extension
 * v2, `update_feed` with `persist`). The extension writes as its participants, never as the user.
 */
class ProductFeedApplier @Inject constructor(
    private val chatMessageRepository: ChatMessageRepository,
    private val messageRevisionRepository: MessageRevisionRepository,
    private val chatRoomRepository: ChatRoomRepository,
) {
    /** [save] stores a new message the way the chat engine stores every message. */
    suspend fun apply(
        extensionId: ChatExtensionId,
        chatId: ChatId,
        update: ProductFeedUpdateModel,
        decoder: CustomContentDecoder,
        save: suspend (ChatMessage) -> Unit,
    ): Result<Unit> = runCancellableCatching {
        if (!chatId.isExtensionChat(extensionId) || !chatRoomRepository.roomExists(chatId)) throw ProductFeedRejection.UnknownRoom()

        when (update) {
            is ProductFeedUpdateModel.New -> addNew(extensionId, chatId, update.message, decoder, save)
            is ProductFeedUpdateModel.Edited -> edit(chatId, update, decoder)
            is ProductFeedUpdateModel.Deleted -> delete(chatId, update.messageId, decoder)
            is ProductFeedUpdateModel.ReactionPlaced -> react(extensionId, chatId, update.messageId, update.participantId, update.emoji, decoder, placed = true)
            is ProductFeedUpdateModel.ReactionRemoved ->
                react(extensionId, chatId, update.messageId, update.participantId, update.emoji, decoder, placed = false)
            // The status of a message of the product means nothing to the user of a room that the host keeps.
            is ProductFeedUpdateModel.StatusChanged -> requireLive(chatId, update.messageId, decoder)
        }
    }

    private suspend fun addNew(
        extensionId: ChatExtensionId,
        chatId: ChatId,
        message: ProductRoomMessage,
        decoder: CustomContentDecoder,
        save: suspend (ChatMessage) -> Unit,
    ) {
        if (message.participantId == ProductParticipant.USER) throw ProductFeedRejection.NotOwner()
        if (find(chatId, message.messageId, decoder) != null) throw ProductFeedRejection.DuplicateMessage()

        save(
            ChatMessage(
                id = ProductMessageIds.stored(message.messageId, chatId),
                chatId = chatId,
                timestamp = message.timestamp,
                origin = ChatMessageOrigin.Extension(extensionId),
                content = message.content,
                status = ChatMessage.Status.NEW,
                replyToMessageId = message.replyTo?.let { find(chatId, it, decoder)?.id },
                originParticipantId = message.participantId,
            ),
        )
    }

    // The new body replaces the old one in place. One revision row carries the "edited" mark, so a product that
    // streams an answer through many edits does not pile up rows.
    private suspend fun edit(chatId: ChatId, update: ProductFeedUpdateModel.Edited, decoder: CustomContentDecoder) {
        val message = requireLive(chatId, update.messageId, decoder)
        if (message.origin !is ChatMessageOrigin.Extension) throw ProductFeedRejection.NotOwner()

        chatMessageRepository.updateMessageContent(message.id, update.content, decoder)
        messageRevisionRepository.deleteRevisions(message.id)
        messageRevisionRepository.saveRevision(message.id, update.content, chatId, update.editedAt, decoder)
    }

    private suspend fun delete(chatId: ChatId, messageId: String, decoder: CustomContentDecoder) {
        val message = find(chatId, messageId, decoder) ?: throw ProductFeedRejection.UnknownMessage()
        if (message.content is ChatMessage.Content.Deleted) return
        chatMessageRepository.markMessageDeleted(message.id)
    }

    // The reaction table keeps one row for each origin kind, so the participants of the product share one row an emoji.
    @Suppress("LongParameterList")
    private suspend fun react(
        extensionId: ChatExtensionId,
        chatId: ChatId,
        messageId: String,
        participantId: String,
        emoji: String,
        decoder: CustomContentDecoder,
        placed: Boolean,
    ) {
        if (participantId == ProductParticipant.USER) throw ProductFeedRejection.NotOwner()
        val message = requireLive(chatId, messageId, decoder)
        val reaction = ChatMessageReaction(
            messageId = message.id,
            content = ChatMessageReactionContent(emoji),
            origin = ChatMessageOrigin.Extension(extensionId),
            timestamp = System.currentTimeMillis(),
        )
        if (placed) {
            chatMessageRepository.addReaction(reaction, chatId)
        } else {
            chatMessageRepository.removeMessageReaction(reaction, chatId)
        }
    }

    private suspend fun requireLive(chatId: ChatId, messageId: String, decoder: CustomContentDecoder): ChatMessage {
        val message = find(chatId, messageId, decoder)
        if (message == null || message.content is ChatMessage.Content.Deleted) throw ProductFeedRejection.UnknownMessage()
        return message
    }

    // A product id names a message of this room only, whatever the store holds under the same id elsewhere.
    private suspend fun find(chatId: ChatId, messageId: String, decoder: CustomContentDecoder): ChatMessage? =
        ProductMessageIds.candidates(messageId, chatId).firstNotNullOfOrNull { candidate ->
            chatMessageRepository.getMessageById(chatId, candidate, decoder)?.takeIf { it.chatId == chatId }
        }
}
