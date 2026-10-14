package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.common.utils.RealCoroutineDispatchers
import io.paritytech.polkadotapp.database.dao.ChatMessageCompactionDao
import io.paritytech.polkadotapp.database.dao.ChatMessageDao
import io.paritytech.polkadotapp.database.dao.ChatMessageReactionDao
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.anyLong
import io.paritytech.polkadotapp.test_shared.eq
import kotlinx.coroutines.runBlocking
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify

class CompactionStatusPropagationTest {
    private val chatMessageDao = mock(ChatMessageDao::class.java)
    private val chatMessageCompactionDao = mock(ChatMessageCompactionDao::class.java)
    private val chatMessageReactionDao = mock(ChatMessageReactionDao::class.java)
    private val chatRoomRepository = mock(ChatRoomRepository::class.java)

    private val repository = RealChatMessageRepository(
        chatMessageDao = chatMessageDao,
        chatMessageCompactionDao = chatMessageCompactionDao,
        chatMessageReactionDao = chatMessageReactionDao,
        chatRoomRepository = chatRoomRepository,
        coroutineDispatchers = RealCoroutineDispatchers()
    )

    @Test
    fun `propagation to IS_SENT upgrades only statuses ranked below it`() = runBlocking<Unit> {
        repository.propagateStatusToCompactedMessages("commit", ChatMessage.Status.IS_SENT)

        verify(chatMessageCompactionDao).propagateStatusToCompactionDescendants(
            eq("commit"),
            eq(listOf(ChatMessageLocal.Status.PROCESSING, ChatMessageLocal.Status.NEW, ChatMessageLocal.Status.DELIVERY_FAILED)),
            eq(ChatMessageLocal.Status.IS_SENT),
            anyLong()
        )
    }

    @Test
    fun `propagation to IS_READ upgrades every status ranked below it`() = runBlocking<Unit> {
        repository.propagateStatusToCompactedMessages("commit", ChatMessage.Status.IS_READ)

        verify(chatMessageCompactionDao).propagateStatusToCompactionDescendants(
            eq("commit"),
            eq(
                listOf(
                    ChatMessageLocal.Status.PROCESSING,
                    ChatMessageLocal.Status.NEW,
                    ChatMessageLocal.Status.IS_SENT,
                    ChatMessageLocal.Status.DELIVERY_FAILED
                )
            ),
            eq(ChatMessageLocal.Status.IS_READ),
            anyLong()
        )
    }

    @Test
    fun `incoming statuses never propagate`() = runBlocking<Unit> {
        repository.propagateStatusToCompactedMessages("commit", ChatMessage.Status.NEW)
        repository.propagateStatusToCompactedMessages("commit", ChatMessage.Status.PROCESSING)
        repository.propagateStatusToCompactedMessages("commit", ChatMessage.Status.DELIVERY_FAILED)

        verify(chatMessageCompactionDao, never()).propagateStatusToCompactionDescendants(any(), any(), any(), anyLong())
    }
}
