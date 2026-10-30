package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.utils.RealCoroutineDispatchers
import io.paritytech.polkadotapp.database.dao.ChatMessageCompactionDao
import io.paritytech.polkadotapp.database.dao.ChatMessageDao
import io.paritytech.polkadotapp.database.dao.ChatMessageReactionDao
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import kotlinx.coroutines.runBlocking
import org.junit.Test

class CompactionStatusPropagationTest {
    private val chatMessageDao = mockk<ChatMessageDao>()
    private val chatMessageCompactionDao = mockk<ChatMessageCompactionDao>(relaxUnitFun = true)
    private val chatMessageReactionDao = mockk<ChatMessageReactionDao>()
    private val chatRoomRepository = mockk<ChatRoomRepository>()

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

        coVerify(exactly = 1) {
            chatMessageCompactionDao.propagateStatusToCompactionDescendants(
                "commit",
                listOf(
                    ChatMessageLocal.Status.PROCESSING,
                    ChatMessageLocal.Status.NEW,
                    ChatMessageLocal.Status.DELIVERY_FAILED
                ),
                ChatMessageLocal.Status.IS_SENT,
                any()
            )
        }
    }

    @Test
    fun `propagation to IS_READ upgrades every status ranked below it`() = runBlocking<Unit> {
        repository.propagateStatusToCompactedMessages("commit", ChatMessage.Status.IS_READ)

        coVerify(exactly = 1) {
            chatMessageCompactionDao.propagateStatusToCompactionDescendants(
                "commit",
                listOf(
                    ChatMessageLocal.Status.PROCESSING,
                    ChatMessageLocal.Status.NEW,
                    ChatMessageLocal.Status.IS_SENT,
                    ChatMessageLocal.Status.DELIVERY_FAILED
                ),
                ChatMessageLocal.Status.IS_READ,
                any()
            )
        }
    }

    @Test
    fun `incoming statuses never propagate`() = runBlocking<Unit> {
        repository.propagateStatusToCompactedMessages("commit", ChatMessage.Status.NEW)
        repository.propagateStatusToCompactedMessages("commit", ChatMessage.Status.PROCESSING)
        repository.propagateStatusToCompactedMessages("commit", ChatMessage.Status.DELIVERY_FAILED)

        coVerify(exactly = 0) {
            chatMessageCompactionDao.propagateStatusToCompactionDescendants(any(), any(), any(), any())
        }
    }
}
