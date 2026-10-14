package io.paritytech.polkadotapp.feature_chats_impl.data.notifications

import io.paritytech.polkadotapp.database.dao.MessageNotificationSentDao
import io.paritytech.polkadotapp.database.model.MessageNotificationSentLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import javax.inject.Inject

interface ChatMessageNotificationSentRepository {
    suspend fun wasNotificationSent(messageId: ChatMessageId): Boolean

    suspend fun saveNotificationSent(messageId: ChatMessageId)
}

class RealChatMessageNotificationSentRepository @Inject constructor(
    private val messageNotificationSentDao: MessageNotificationSentDao
) : ChatMessageNotificationSentRepository {
    override suspend fun wasNotificationSent(messageId: ChatMessageId) =
        messageNotificationSentDao.wasNotificationSent(messageId)

    override suspend fun saveNotificationSent(messageId: ChatMessageId) {
        messageNotificationSentDao.insert(MessageNotificationSentLocal(messageId, System.currentTimeMillis()))
    }
}
