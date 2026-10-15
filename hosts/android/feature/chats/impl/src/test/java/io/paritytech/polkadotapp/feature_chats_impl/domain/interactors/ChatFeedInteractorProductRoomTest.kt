package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import android.net.Uri
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomActivityWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.ProductAttachmentEncoder
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatMessageSource
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomSourceContext
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomSourceContextFactory
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceAction
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceActionOutcome
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicyProvider
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

/** The product-room paths of the room interactor (P8.6). */
class ChatFeedInteractorProductRoomTest {
    private val roomId = ChatId.forExtensionRoom("ProductBot_weather", "lobby")
    private val uri = mockk<Uri>()
    private val media = ChatMediaItemModel.Image(
        ChatResourceModel.Inline("image/jpeg", byteArrayOf(1).toDataByteArray()),
        ChatMediaMetaModel("image/jpeg", sizeBytes = 1, width = 1, height = 1, placeholder = null, alt = null),
    )
    private val activityWriter = mockk<ChatRoomActivityWriter>(relaxed = true)
    private val chatEngine = mockk<ChatEngine>(relaxed = true)
    private val productSource = mockk<ChatMessageSource>(relaxed = true)
    private val roomSource = mockk<ChatRoomSourceContext>(relaxed = true)
    private val roomSourceContextFactory = mockk<ChatRoomSourceContextFactory> {
        every { create(any()) } returns roomSource
    }
    private val roomPolicyProvider = mockk<ChatRoomPolicyProvider> {
        every { observePolicy(any()) } returns flowOf(ChatRoomPolicy.Defaults)
    }
    private val productAttachmentEncoder = mockk<ProductAttachmentEncoder> {
        coEvery { encode(any(), any(), any()) } returns Result.success(media)
    }

    @Test
    fun `closing a room clears its activity line`() {
        interactor().closeRoom(roomId)

        verify { activityWriter.set(roomId, null) }
        verify { roomSource.close() }
    }

    @Test
    fun `an attachment in a product-source room goes to the product as inline media`() = runBlocking<Unit> {
        coEvery { roomSource.productSource() } returns productSource
        coEvery { productSource.perform(any()) } returns ChatSourceActionOutcome.Done

        val outcome = interactor().sendAttachmentMessage(roomId, uri, "image/jpeg", 1.bytes, text = "Look", replyToMessageId = "m0")

        assertEquals(ChatSourceActionOutcome.Done, outcome.getOrThrow())
        coVerify {
            productSource.perform(
                match { action ->
                    val send = action as ChatSourceAction.Send
                    val body = (send.content as ChatMessage.Content.ProductBody).body as ProductMessageBody.RichText
                    send.replyTo == "m0" && body.text == "Look" && body.media == listOf(media)
                },
            )
        }
    }

    @Test
    fun `an attachment in a host-store room is saved as a message of the user`() = runBlocking<Unit> {
        coEvery { roomSource.productSource() } returns null

        interactor().sendAttachmentMessage(roomId, uri, "image/jpeg", 1.bytes, text = null, replyToMessageId = null)

        coVerify {
            chatEngine.sendUserMessage(
                messageId = any(),
                chatId = roomId,
                content = match { (it as ChatMessage.Content.ProductBody).body == ProductMessageBody.RichText(null, listOf(media)) },
                replyToMessageId = null,
                onSaved = any(),
            )
        }
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
        roomPolicyProvider = roomPolicyProvider,
        productRoomUserActions = mockk(relaxed = true),
        roomSourceContextFactory = roomSourceContextFactory,
        productAttachmentEncoder = productAttachmentEncoder,
        activityRepository = mockk(relaxed = true),
        activityWriter = activityWriter,
    )
}
