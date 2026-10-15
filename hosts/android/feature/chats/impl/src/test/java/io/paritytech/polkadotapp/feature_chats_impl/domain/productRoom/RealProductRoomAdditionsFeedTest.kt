package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.database.dao.ChatRoomDao
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatThreadParentModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageIds
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ProductMessageCaching
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ProductRoomCacheEntry
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class RealProductRoomAdditionsFeedTest {
    private val extensionId = "ProductBot_sample.dot"
    private val parent = ChatId.forExtensionRoom(extensionId, "host")
    private val thread = ChatId.forExtensionRoom(extensionId, "thread-host-m1")

    private val chatRoomRepository = mockk<ChatRoomRepository> {
        coEvery { getRoomMetadata(parent) } returns RoomMetadata(name = "Sample: host store", icon = null)
    }
    private val chatMessageRepository = mockk<ChatMessageRepository>()
    private val declarations = mockk<ChatRoomDeclarationsRepository>()
    private val cache = mockk<ProductMessageCaching>()

    private val feed = RealProductRoomAdditionsFeed(
        personaRepository = mockk(),
        metaRepository = mockk(),
        preparing = PreparingProductMessages(),
        chatRoomRepository = chatRoomRepository,
        chatMessageRepository = chatMessageRepository,
        declarationsRepository = declarations,
        productMessageCache = cache,
        eventPublisher = mockk(),
    )

    @Test
    fun `the root of a host-store parent is its stored message`() = runBlocking<Unit> {
        withProductSource(false)
        val stored = ChatMessage(
            id = ProductMessageIds.stored("m1", parent),
            chatId = parent,
            timestamp = 1,
            origin = ChatMessageOrigin.Extension(extensionId),
            content = ChatMessage.Content.Text("Plain text message"),
            status = ChatMessage.Status.IS_READ,
        )
        coEvery { chatMessageRepository.getMessageById(parent, any(), any()) } returns null
        coEvery { chatMessageRepository.getMessageById(parent, stored.id, any()) } returns stored

        val root = feed.threadRoot(thread, ChatThreadParentModel("host", "m1"))

        assertEquals(ChatThreadRootModel(parent, "Sample: host store", ChatQuoteText("Plain text message", markdown = false)), root)
    }

    @Test
    fun `a product-source root outside the kept page is an earlier message`() = runBlocking<Unit> {
        withProductSource(true)
        coEvery { cache.entry(parent) } returns ProductRoomCacheEntry(
            messages = listOf(productMessage("other")),
            next = null,
            seenMessageId = null,
            seenTimestamp = null,
            reportedSeenMessageId = null,
        )

        assertEquals(null, feed.threadRoot(thread, ChatThreadParentModel("host", "m1")).rootText)
    }

    @Test
    fun `a thread room with unread messages is in the unread set`() = runBlocking<Unit> {
        every { chatRoomRepository.subscribeChatSummaries() } returns flowOf(
            listOf(summary(thread, unseen = 2), summary(parent, unseen = 5)),
        )

        assertEquals(setOf(thread), feed.roomsWithUnread(setOf(thread)).first())
    }

    private fun withProductSource(productSource: Boolean) {
        coEvery { declarations.get(parent) } returns
            ChatRoomDeclarations.EMPTY.copy(config = ChatRoomConfigModel.HOST_DEFAULT.copy(productSource = productSource))
    }

    private fun productMessage(id: String) = ProductRoomMessage(
        messageId = id,
        participantId = "product",
        timestamp = 1,
        content = ChatMessage.Content.Text(id),
        status = ProductMessageStatusModel.SENT,
        replyTo = null,
        editedAt = null,
        reactions = emptyList(),
    )

    private fun summary(chatId: ChatId, unseen: Int) = ChatRoomDao.ChatRoomSummaryLocal(
        chatId = chatId.value.value,
        createdAt = 0,
        roomName = null,
        roomIcon = null,
        unseenCount = unseen,
        hasUnseenReaction = false,
        lastMessage = null,
    )
}
