package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import android.net.Uri
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.spyk
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
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentDraft
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatMessageSource
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomSourceContext
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomSourceContextFactory
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceAction
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceActionOutcome
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentStager
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentStagingGate
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.PublicStorageNotice
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicyProvider
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.coroutines.Dispatchers
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

    // A spy keeps the real size rule of staging.
    private val productAttachmentEncoder = spyk(
        ProductAttachmentEncoder(mockk(), mockk(), { _, _ -> null }, TestCoroutineDispatchers(Dispatchers.Unconfined)),
    ) {
        coEvery { encode(any(), any(), any()) } returns Result.success(media)
    }
    private val attachmentStager = mockk<ProductAttachmentStager>(relaxed = true) {
        every { observe(any()) } returns flowOf(emptyList())
    }
    private val publicStorageNotice = mockk<PublicStorageNotice>(relaxed = true)
    private val stagingGate = mockk<ProductAttachmentStagingGate> {
        every { isServed } returns true
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
    fun `an attachment above 1 MiB in a room that stages attachments goes to the outbox`() = runBlocking<Unit> {
        val draft = mockk<ProductAttachmentDraft>()
        val size = (ProductAttachmentEncoder.LIMIT_BYTES + 1).bytes
        every { roomPolicyProvider.observePolicy(roomId) } returns flowOf(stagingRoom)
        coEvery { productAttachmentEncoder.prepareStaged(uri, "application/pdf", size) } returns Result.success(draft)

        val outcome = interactor().sendAttachmentMessage(roomId, uri, "application/pdf", size, text = "Report", replyToMessageId = "m0")

        assertEquals(ChatSourceActionOutcome.Done, outcome.getOrThrow())
        coVerify { attachmentStager.enqueue(roomId, draft, "Report", "m0") }
        coVerify(exactly = 0) { productAttachmentEncoder.encode(any(), any(), any()) }
    }

    @Test
    fun `a host that does not serve staging sends an attachment above 1 MiB inline in a room that declares it`() = runBlocking<Unit> {
        coEvery { roomSource.productSource() } returns null
        every { roomPolicyProvider.observePolicy(roomId) } returns flowOf(stagingRoom)
        every { stagingGate.isServed } returns false
        val size = (ProductAttachmentEncoder.LIMIT_BYTES + 1).bytes

        interactor().sendAttachmentMessage(roomId, uri, "image/jpeg", size, text = null, replyToMessageId = null)

        assertEquals(null, interactor().publicStorageNoticeFor(roomId, "image/jpeg", size))
        coVerify { productAttachmentEncoder.encode(uri, "image/jpeg", size) }
        coVerify(exactly = 0) { attachmentStager.enqueue(any(), any(), any(), any()) }
    }

    @Test
    fun `an attachment above 1 MiB in a room that does not stage goes inline as before`() = runBlocking<Unit> {
        coEvery { roomSource.productSource() } returns null
        val size = (ProductAttachmentEncoder.LIMIT_BYTES + 1).bytes

        interactor().sendAttachmentMessage(roomId, uri, "image/jpeg", size, text = null, replyToMessageId = null)

        coVerify { productAttachmentEncoder.encode(uri, "image/jpeg", size) }
        coVerify(exactly = 0) { attachmentStager.enqueue(any(), any(), any(), any()) }
    }

    @Test
    fun `the public-storage notice shows one time for each product before the first staged attachment`() = runBlocking<Unit> {
        val size = (ProductAttachmentEncoder.LIMIT_BYTES + 1).bytes
        every { roomPolicyProvider.observePolicy(roomId) } returns flowOf(stagingRoom)
        coEvery { chatEngine.getDefaultChatExtensionDisplay(roomId) } returns Result.success(ChatDisplay("Weather", ChatAvatar.Account("Weather", byteArrayOf())))
        every { publicStorageNotice.isAccepted("ProductBot_weather") } returns false
        val interactor = interactor()

        assertEquals("Weather", interactor.publicStorageNoticeFor(roomId, "application/pdf", size))
        assertEquals(null, interactor.publicStorageNoticeFor(roomId, "application/pdf", 1.bytes))

        interactor.acceptPublicStorageNotice(roomId)
        verify { publicStorageNotice.accept("ProductBot_weather") }
        every { publicStorageNotice.isAccepted("ProductBot_weather") } returns true
        assertEquals(null, interactor.publicStorageNoticeFor(roomId, "application/pdf", size))
    }

    @Test
    fun `retry and delete of an outbox entry go to the stager`() = runBlocking<Unit> {
        coEvery { attachmentStager.retry("outbox:1") } returns true

        assertEquals(ChatSourceActionOutcome.Done, interactor().retryProductRoomMessage(roomId, "outbox:1").getOrThrow())
        assertEquals(ChatSourceActionOutcome.Done, interactor().deleteProductRoomMessage(roomId, "outbox:1", ChatRoomPolicy.Defaults).getOrThrow())

        coVerify { attachmentStager.retry("outbox:1") }
        coVerify { attachmentStager.delete("outbox:1") }
        coVerify(exactly = 0) { roomSource.productSource() }
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

    private val stagingRoom = ChatRoomPolicy.Defaults.copy(
        features = ChatRoomPolicy.Defaults.features.copy(preimageAttachments = true),
    )

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
        attachmentStager = attachmentStager,
        publicStorageNotice = publicStorageNotice,
        stagingGate = stagingGate,
    )
}
