package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.mockk.coEvery
import io.mockk.mockk
import io.mockk.slot
import io.paritytech.polkadotapp.common.utils.RealCoroutineDispatchers
import io.paritytech.polkadotapp.database.dao.ChatMessageDao
import io.paritytech.polkadotapp.database.dao.ChatMessageDao.MessageContentUpdateLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class ChatMessageContentUpdateTest {
    private val chatMessageDao: ChatMessageDao = mockk()

    private val repository = RealChatMessageRepository(
        chatMessageDao = chatMessageDao,
        chatMessageCompactionDao = mockk(),
        chatMessageReactionDao = mockk(),
        chatRoomRepository = mockk(),
        coroutineDispatchers = RealCoroutineDispatchers(),
    )

    @Test
    fun `a new body of a message also replaces the text that the search reads`() = runBlocking<Unit> {
        val updates = slot<List<MessageContentUpdateLocal>>()
        coEvery { chatMessageDao.updateMessagesContents(capture(updates)) } returns Unit

        repository.updateMessageContent("m1", ChatMessage.Content.Text("Edited by the product"), AlwaysFailCustomContentDecoder())

        assertEquals(listOf("Edited by the product"), updates.captured.map { it.searchableContent })
    }
}
