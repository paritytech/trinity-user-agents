package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatMessageSource
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomSourceContext
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomSourceContextFactory
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceAction
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceActionOutcome
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

/** The paths of the room interactor for a room whose product keeps the messages. */
class ChatFeedInteractorProductRoomTest {
    private val roomId = ChatId.forExtensionRoom("ProductBot_weather", "lobby")

    private val chatEngine = mockk<ChatEngine>(relaxed = true)
    private val productSource = mockk<ChatMessageSource>(relaxed = true) {
        coEvery { perform(any()) } returns ChatSourceActionOutcome.Done
    }
    private val roomSource = mockk<ChatRoomSourceContext>(relaxed = true)
    private val roomSourceContextFactory = mockk<ChatRoomSourceContextFactory> {
        every { create(any()) } returns roomSource
    }

    @Test
    fun `closing a room closes its source`() {
        interactor().closeRoom(roomId)

        verify { roomSource.close() }
    }

    @Test
    fun `a text in a room the product keeps goes to the product, not to the host database`() = runBlocking<Unit> {
        coEvery { roomSource.productSource() } returns productSource

        val outcome = interactor().sendTextMessage(roomId, "hello", replyToMessageId = "m0")

        assertEquals(ChatSourceActionOutcome.Done, outcome)
        coVerify {
            productSource.perform(match { it is ChatSourceAction.Send && it.replyTo == "m0" && (it.content as ChatMessage.Content.Text).text == "hello" })
        }
        coVerify(exactly = 0) { chatEngine.sendUserMessage(any(), any(), any(), any(), any()) }
    }

    @Test
    fun `an edit, a reaction and a seen message in a room the product keeps go to the product`() = runBlocking<Unit> {
        coEvery { roomSource.productSource() } returns productSource
        val interactor = interactor()

        interactor.sendEdit(roomId, "m1", "new")
        interactor.toggleUserReaction(roomId, "m1", "👍", emptyList())
        interactor.reportMessageSeen(roomId, "m1")

        coVerify {
            productSource.perform(match { it is ChatSourceAction.Edit && it.messageId == "m1" && (it.content as ChatMessage.Content.Text).text == "new" })
        }
        coVerify { productSource.perform(ChatSourceAction.ToggleReaction("m1", "👍")) }
        coVerify { productSource.perform(ChatSourceAction.MarkSeen(setOf("m1"))) }
        coVerify(exactly = 0) { chatEngine.editMessage(any(), any(), any()) }
    }

    @Test
    fun `a refusal of the product reaches the room screen`() = runBlocking<Unit> {
        coEvery { roomSource.productSource() } returns productSource
        coEvery { productSource.perform(any()) } returns ChatSourceActionOutcome.Refused("Locked")

        assertEquals(ChatSourceActionOutcome.Refused("Locked"), interactor().sendEdit(roomId, "m1", "new"))
    }

    @Test
    fun `a room that the host database keeps writes through the chat engine`() = runBlocking<Unit> {
        coEvery { roomSource.productSource() } returns null
        val interactor = interactor()

        interactor.sendTextMessage(roomId, "hello", replyToMessageId = null)
        interactor.sendEdit(roomId, "m1", "new")

        coVerify { chatEngine.sendUserMessage(chatId = roomId, content = any(), replyToMessageId = null, messageId = any(), onSaved = any()) }
        coVerify { chatEngine.editMessage(roomId, "m1", "new") }
    }

    private fun interactor() = ChatFeedInteractor(
        chatMessageRepository = mockk(relaxed = true),
        chatDraftRepository = mockk(relaxed = true),
        chatSessionManager = mockk(relaxed = true),
        chatNotificationPublisher = mockk(relaxed = true),
        chatActiveTracker = mockk(relaxed = true),
        chatEngine = chatEngine,
        contactsRepository = mockk(relaxed = true),
        coroutineDispatchers = mockk(relaxed = true),
        chatDisplayGenerator = mockk(relaxed = true),
        dmChatMessageOriginDisplayResolver = mockk(relaxed = true),
        accountRepository = mockk(relaxed = true),
        addContactUseCase = mockk(relaxed = true),
        removeContactUseCase = mockk(relaxed = true),
        incomingChatRequestProcessor = mockk(relaxed = true),
        callController = mockk(relaxed = true),
        callStateTracker = mockk(relaxed = true),
        fileUploadRepository = mockk(relaxed = true),
        fileUploadStarter = mockk(relaxed = true),
        attachmentMetaBuilder = mockk(relaxed = true),
        attachmentFileStorage = mockk(relaxed = true),
        hopNodeUrlProvider = mockk(relaxed = true),
        roomSourceContextFactory = roomSourceContextFactory,
    )
}
