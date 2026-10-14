package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.database.dao.MessageRevisionDao
import io.paritytech.polkadotapp.database.model.MessageRevisionLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.MessageRevision
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatMessageContentLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.toDomain
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toEncodedByteArray
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocalType
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

interface MessageRevisionRepository {
    suspend fun saveRevision(
        messageId: ChatMessageId,
        content: ChatMessage.Content,
        chatId: ChatId,
        timestamp: Long,
        customContentDecoder: CustomContentDecoder
    )

    fun subscribeRevisions(
        chatId: ChatId,
        customContentDecoder: CustomContentDecoder
    ): Flow<Map<ChatMessageId, MessageRevision>>

    suspend fun getRevisionsForMessage(
        messageId: ChatMessageId,
        customContentDecoder: CustomContentDecoder
    ): List<MessageRevision>
}

class RealMessageRevisionRepository @Inject constructor(
    private val messageRevisionDao: MessageRevisionDao
) : MessageRevisionRepository {
    override suspend fun saveRevision(
        messageId: ChatMessageId,
        content: ChatMessage.Content,
        chatId: ChatId,
        timestamp: Long,
        customContentDecoder: CustomContentDecoder
    ) {
        val local = MessageRevisionLocal(
            messageId = messageId,
            type = content.toLocalType(),
            content = content.toEncodedByteArray(customContentDecoder),
            chatId = chatId.toLocal(),
            timestamp = timestamp
        )
        messageRevisionDao.insert(local)
    }

    override fun subscribeRevisions(
        chatId: ChatId,
        customContentDecoder: CustomContentDecoder
    ): Flow<Map<ChatMessageId, MessageRevision>> {
        return messageRevisionDao.subscribeLatestRevisions(chatId.toLocal())
            .map { revisions ->
                revisions.associate { it.messageId to it.toDomain(customContentDecoder) }
            }
    }

    override suspend fun getRevisionsForMessage(
        messageId: ChatMessageId,
        customContentDecoder: CustomContentDecoder
    ): List<MessageRevision> {
        return messageRevisionDao.getRevisionsForMessage(messageId).map {
            it.toDomain(customContentDecoder)
        }
    }

    private fun MessageRevisionLocal.toDomain(
        customContentDecoder: CustomContentDecoder
    ): MessageRevision {
        val contentLocal = BinaryScale.decodeFromByteArray<ChatMessageContentLocal>(content)
        val chatId = ChatId.fromRawValue(chatId)

        return MessageRevision(
            messageId = messageId,
            content = contentLocal.toDomain(customContentDecoder),
            chatId = chatId,
            timestamp = timestamp
        )
    }
}
