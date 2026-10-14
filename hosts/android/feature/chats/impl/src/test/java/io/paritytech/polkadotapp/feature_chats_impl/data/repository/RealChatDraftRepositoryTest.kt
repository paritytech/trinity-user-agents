package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.common.utils.RealCoroutineDispatchers
import io.paritytech.polkadotapp.database.dao.ChatDraftDao
import io.paritytech.polkadotapp.database.model.ChatDraftLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDraft
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDraftRelation
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RealChatDraftRepositoryTest {
    private val chatId = ChatId.fromRawValue(byteArrayOf(1, 2, 3))

    private val chatDraftDao = FakeChatDraftDao()

    private val repository = RealChatDraftRepository(chatDraftDao, RealCoroutineDispatchers())

    @Test
    fun `save and get round-trip for None relation`() = runBlocking<Unit> {
        val draft = ChatDraft(text = "hello", relation = ChatDraftRelation.None)

        repository.saveDraft(chatId, draft)

        assertEquals(draft, repository.getDraft(chatId))
    }

    @Test
    fun `save and get round-trip for Reply relation`() = runBlocking<Unit> {
        val draft = ChatDraft(text = "reply", relation = ChatDraftRelation.Reply(messageId = "message-1"))

        repository.saveDraft(chatId, draft)

        assertEquals(draft, repository.getDraft(chatId))
    }

    @Test
    fun `save and get round-trip for Edit relation`() = runBlocking<Unit> {
        val draft = ChatDraft(
            text = "edited",
            relation = ChatDraftRelation.Edit(messageId = "message-2", originalText = "original")
        )

        repository.saveDraft(chatId, draft)

        assertEquals(draft, repository.getDraft(chatId))
    }

    @Test
    fun `deleteDraft removes the row`() = runBlocking<Unit> {
        repository.saveDraft(chatId, ChatDraft(text = "temp", relation = ChatDraftRelation.None))

        repository.deleteDraft(chatId)

        assertNull(repository.getDraft(chatId))
    }
}

private class FakeChatDraftDao : ChatDraftDao {
    private val drafts = MutableStateFlow<Map<String, ChatDraftLocal>>(emptyMap())

    override suspend fun upsert(draft: ChatDraftLocal) {
        drafts.update { it + (draft.chatId.key() to draft) }
    }

    override suspend fun deleteByChatId(chatId: ByteArray) {
        drafts.update { it - chatId.key() }
    }

    override suspend fun getDraft(chatId: ByteArray): ChatDraftLocal? {
        return drafts.value[chatId.key()]
    }

    override fun subscribeDrafts(): Flow<List<ChatDraftLocal>> {
        return drafts.map { it.values.toList() }
    }

    private fun ByteArray.key(): String = joinToString(separator = ",")
}
