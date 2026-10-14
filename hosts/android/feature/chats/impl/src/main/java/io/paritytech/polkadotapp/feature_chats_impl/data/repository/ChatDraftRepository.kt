package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.database.dao.ChatDraftDao
import io.paritytech.polkadotapp.database.model.ChatDraftLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toChatDraftRelation
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toScaleBytes
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDraft
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocal
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import javax.inject.Inject

interface ChatDraftRepository {
    suspend fun getDraft(chatId: ChatId): ChatDraft?

    suspend fun saveDraft(chatId: ChatId, draft: ChatDraft)

    suspend fun deleteDraft(chatId: ChatId)

    fun subscribeDrafts(): Flow<Map<ChatId, ChatDraft>>
}

class RealChatDraftRepository @Inject constructor(
    private val chatDraftDao: ChatDraftDao,
    private val coroutineDispatchers: CoroutineDispatchers,
) : ChatDraftRepository {
    override suspend fun getDraft(chatId: ChatId): ChatDraft? = withContext(coroutineDispatchers.io) {
        chatDraftDao.getDraft(chatId.toLocal())?.toDomain()
    }

    override suspend fun saveDraft(chatId: ChatId, draft: ChatDraft) = withContext(coroutineDispatchers.io) {
        chatDraftDao.upsert(draft.toLocal(chatId))
    }

    override suspend fun deleteDraft(chatId: ChatId) = withContext(coroutineDispatchers.io) {
        chatDraftDao.deleteByChatId(chatId.toLocal())
    }

    override fun subscribeDrafts(): Flow<Map<ChatId, ChatDraft>> {
        return chatDraftDao.subscribeDrafts()
            .map { drafts ->
                drafts.associate { local -> ChatId.fromRawValue(local.chatId) to local.toDomain() }
            }
            .flowOn(coroutineDispatchers.io)
    }

    private fun ChatDraftLocal.toDomain(): ChatDraft =
        ChatDraft(text = text, relation = relation.toChatDraftRelation())

    private fun ChatDraft.toLocal(chatId: ChatId): ChatDraftLocal =
        ChatDraftLocal(
            chatId = chatId.toLocal(),
            text = text,
            relation = relation.toScaleBytes(),
        )
}
