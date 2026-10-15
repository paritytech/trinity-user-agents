package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedRejection
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageIds
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.UnknownChatRoomException
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatMessageMetaWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomCallWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.CustomContentDecoder
import javax.inject.Inject

/** The most messages of a product-source room whose meta the host keeps (07-LATER §3.5). */
const val PRODUCT_SOURCE_META_KEPT = 2_000

/**
 * Applies the Chat Extension v2 additions that an extension sets for its rooms: the features, the meta of
 * messages and the call. The host keeps each value also when it does not serve its feature, so a later build
 * applies it.
 */
class ProductRoomAdditions @Inject constructor(
    private val chatRoomRepository: ChatRoomRepository,
    private val chatMessageRepository: ChatMessageRepository,
    private val declarationsRepository: ChatRoomDeclarationsRepository,
    private val declarationsWriter: ChatRoomDeclarationsWriter,
    private val metaWriter: ChatMessageMetaWriter,
    private val callWriter: ChatRoomCallWriter,
) {
    suspend fun setFeatures(extensionId: ChatExtensionId, chatId: ChatId, features: ChatRoomFeaturesModel): Result<Unit> =
        runCancellableCatching {
            requireOwnRoom(extensionId, chatId) { UnknownChatRoomException() }
            declarationsWriter.setFeatures(chatId, features)
        }

    /**
     * The product keeps the messages of a product-source room, so the host cannot check the message id there. It
     * orders the meta of such a room by the time of the call, and keeps the newest [PRODUCT_SOURCE_META_KEPT].
     *
     * A reasoning row is not counted as unread (07-LATER §9.4), so the host marks a stored reasoning message as read.
     */
    suspend fun setMessageMeta(
        extensionId: ChatExtensionId,
        chatId: ChatId,
        messageId: String,
        meta: ChatMessageMetaModel,
        decoder: CustomContentDecoder,
    ): Result<Unit> = runCancellableCatching {
        requireOwnRoom(extensionId, chatId) { ProductFeedRejection.UnknownRoom() }
        val productSource = declarationsRepository.get(chatId)?.effectiveConfig?.productSource == true
        val timestamp = if (productSource) {
            System.currentTimeMillis()
        } else {
            val stored = storedMessage(chatId, messageId, decoder) ?: throw ProductFeedRejection.UnknownMessage()
            if (meta.reasoning != null && stored.status == ChatMessage.Status.NEW) chatMessageRepository.markMessageAsRead(stored.id)
            stored.timestamp
        }

        when {
            meta.isEmpty -> metaWriter.delete(chatId, messageId)
            productSource -> metaWriter.put(chatId, messageId, timestamp, meta, keepNewest = PRODUCT_SOURCE_META_KEPT)
            else -> metaWriter.put(chatId, messageId, timestamp, meta, keepNewest = null)
        }
    }

    suspend fun setCall(extensionId: ChatExtensionId, chatId: ChatId, call: ChatRoomCallModel?): Result<Unit> =
        runCancellableCatching {
            requireOwnRoom(extensionId, chatId) { UnknownChatRoomException() }
            callWriter.set(chatId, call)
        }

    private suspend fun requireOwnRoom(extensionId: ChatExtensionId, chatId: ChatId, unknown: () -> Throwable) {
        if (!chatId.isExtensionChat(extensionId) || !chatRoomRepository.roomExists(chatId)) throw unknown()
    }

    // A product id names a message of this room only. A deleted message has no meta.
    private suspend fun storedMessage(chatId: ChatId, messageId: String, decoder: CustomContentDecoder): ChatMessage? =
        ProductMessageIds.candidates(messageId, chatId).firstNotNullOfOrNull { candidate ->
            chatMessageRepository.getMessageById(chatId, candidate, decoder)?.takeIf { it.chatId == chatId }
        }?.takeUnless { it.content is ChatMessage.Content.Deleted }
}
