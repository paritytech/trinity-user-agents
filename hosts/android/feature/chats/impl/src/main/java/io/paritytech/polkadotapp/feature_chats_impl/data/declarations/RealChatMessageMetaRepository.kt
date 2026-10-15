package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.paritytech.polkadotapp.database.dao.ChatMessageMetaDao
import io.paritytech.polkadotapp.database.model.ChatMessageMetaLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMessageMetaRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageMetaModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocal
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import javax.inject.Inject

/** Writes the meta of messages, by the message id of the product. */
interface ChatMessageMetaWriter {
    /** Keeps the meta of the message. With [keepNewest], the room then keeps only that number of the newest rows. */
    suspend fun put(chatId: ChatId, messageId: String, timestamp: Long, meta: ChatMessageMetaModel, keepNewest: Int?)

    suspend fun delete(chatId: ChatId, messageId: String)

    suspend fun deleteRoom(chatId: ChatId)
}

class RealChatMessageMetaRepository @Inject constructor(
    private val dao: ChatMessageMetaDao,
) : ChatMessageMetaRepository, ChatMessageMetaWriter {
    override fun observe(chatId: ChatId): Flow<Map<String, ChatMessageMetaModel>> =
        dao.observe(chatId.toLocal())
            .map { rows -> rows.mapNotNull { row -> row.meta.decodeMessageMeta()?.let { row.messageId to it } }.toMap() }
            .distinctUntilChanged()

    override suspend fun put(chatId: ChatId, messageId: String, timestamp: Long, meta: ChatMessageMetaModel, keepNewest: Int?) {
        val row = ChatMessageMetaLocal(chatId.toLocal(), messageId, timestamp, meta.encode())
        if (keepNewest == null) dao.put(row) else dao.putAndTrim(row, keepNewest)
    }

    override suspend fun delete(chatId: ChatId, messageId: String) = dao.delete(chatId.toLocal(), messageId)

    override suspend fun deleteRoom(chatId: ChatId) = dao.deleteRoom(chatId.toLocal())
}
