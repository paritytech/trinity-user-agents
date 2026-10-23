package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import android.net.Uri
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.feature_calls_api.domain.CallController
import io.paritytech.polkadotapp.feature_calls_api.domain.CallStateTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatRooms
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatContactStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomContactModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.OpenChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageRevisionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.ProductAttachmentEncoder
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentDraft
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductOutboxEntry
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatMessageSource
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomSourceContext
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomSourceContextFactory
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceAction
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceActionOutcome
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatUserInputState
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.InitiateCallResult
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentStager
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments.FakeProductChatPaymentStore
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicyProvider
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ProductRoomUserActions
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The paths of the room interactor for a room of the chat product. */
class ChatFeedInteractorProductRoomTest {
    private val roomId = ChatId.forExtensionRoom("ProductBot_chat.dot", "dm-01")
    private val uri = mockk<Uri>()
    private val contact = ChatRoomContactModel(ByteArray(32) { 4 }.intoAccountId(), "bob", ChatContactStateModel.ESTABLISHED)

    private val chatEngine = mockk<ChatEngine>(relaxed = true)
    private val roomSource = mockk<ChatRoomSourceContext>(relaxed = true)
    private val roomSourceContextFactory = mockk<ChatRoomSourceContextFactory> {
        every { create(any()) } returns roomSource
    }
    private val roomPolicyProvider = mockk<ChatRoomPolicyProvider> {
        every { observePolicy(any()) } returns flowOf(contactRoom)
    }
    private val productAttachmentEncoder = mockk<ProductAttachmentEncoder>()
    private val attachmentStager = mockk<ProductAttachmentStager>(relaxed = true) {
        every { observe(any()) } returns flowOf(emptyList())
        coEvery { pending(any()) } returns null
    }
    private val productRoomUserActions = mockk<ProductRoomUserActions> {
        coEvery { contactAction(any(), any()) } returns Result.success(Unit)
    }
    private val personChatRooms = mockk<PersonChatRooms> {
        every { isPersonChatRoom(roomId) } returns true
    }
    private val callController = mockk<CallController>(relaxed = true)
    private val callStateTracker = mockk<CallStateTracker> {
        every { getActiveCall() } returns null
    }

    private val contactRoom: ChatRoomPolicy
        get() = ChatRoomPolicy.Defaults.copy(
            contact = contact,
            features = ChatRoomPolicy.Defaults.features.copy(hopAttachments = true),
        )

    @Test
    fun `closing a room closes its source`() {
        interactor().closeRoom(roomId)

        verify { roomSource.close() }
    }

    @Test
    fun `an attachment in a room of the chat product waits in the outbox for HOP`() = runBlocking<Unit> {
        val draft = mockk<ProductAttachmentDraft>()
        coEvery { productAttachmentEncoder.prepare(uri, "image/jpeg", 1.bytes) } returns Result.success(draft)

        val outcome = interactor().sendAttachmentMessage(roomId, uri, "image/jpeg", 1.bytes, text = "Look", replyToMessageId = "m0")

        assertEquals(ChatSourceActionOutcome.Done, outcome.getOrThrow())
        coVerify { attachmentStager.enqueue(roomId, draft, "Look", "m0") }
    }

    @Test
    fun `a product room that does not take HOP files refuses an attachment`() = runBlocking<Unit> {
        every { roomPolicyProvider.observePolicy(roomId) } returns
            flowOf(contactRoom.copy(features = ChatRoomPolicy.Defaults.features.copy(hopAttachments = false)))

        val outcome = interactor().sendAttachmentMessage(roomId, uri, "image/jpeg", 1.bytes, text = null, replyToMessageId = null)

        assertTrue(outcome.isFailure)
        coVerify(exactly = 0) { attachmentStager.enqueue(any(), any(), any(), any()) }
    }

    @Test
    fun `a call in a room of the chat product opens the native call screen at once`() = runBlocking<Unit> {
        assertEquals(InitiateCallResult.DONE, interactor().initiateCall(roomId, "bob", withVideo = true))

        verify { callController.initiateCall(roomId, "bob", true) }
    }

    @Test
    fun `a text in a room the product keeps waits in the outbox, and the room shows its newest messages`() = runBlocking<Unit> {
        every { roomPolicyProvider.observePolicy(roomId) } returns
            flowOf(contactRoom.copy(messageStore = ChatRoomPolicy.MessageStore.PRODUCT))
        val source = mockk<ChatMessageSource>(relaxed = true)
        coEvery { roomSource.productSource() } returns source

        val outcome = interactor().sendTextMessage(roomId, "hello", replyToMessageId = "m0", request = true)

        assertEquals(ChatSourceActionOutcome.Done, outcome)
        coVerify { attachmentStager.enqueueText(roomId, "hello", "m0", true) }
        coVerify { source.perform(ChatSourceAction.ShowNewest) }
    }

    @Test
    fun `an edit and a reaction on a message in the outbox wait there`() = runBlocking<Unit> {
        coEvery { attachmentStager.editPending("p1", "new", any()) } returns true
        coEvery { attachmentStager.toggleReactionPending("p1", "👍") } returns true

        val interactor = interactor()
        assertEquals(ChatSourceActionOutcome.Done, interactor.sendEdit(roomId, "p1", "new"))
        assertEquals(ChatSourceActionOutcome.Done, interactor.toggleUserReaction(roomId, "p1", "👍", emptyList()))

        coVerify(exactly = 0) { roomSource.productSource() }
    }

    @Test
    fun `a room of the chat product shows no input of the product before its contact is declared`() = runBlocking<Unit> {
        every { roomPolicyProvider.observePolicy(roomId) } returns flowOf(ChatRoomPolicy.Defaults)

        val input = interactor().subscribeUserInputState(OpenChatRequest.ExistingChat(roomId)).first()

        assertEquals(ChatUserInputState.Nothing, input)
    }

    @Test
    fun `block, unblock, leave and the answer to a request go to the product`() = runBlocking<Unit> {
        val interactor = interactor()

        interactor.blockUser(roomId)
        interactor.unblockUser(roomId)
        interactor.leaveChat(roomId)
        interactor.answerProductChatRequest(roomId, accept = true)
        interactor.answerProductChatRequest(roomId, accept = false)

        for (action in listOf(
            ChatUserActionModel.Block,
            ChatUserActionModel.Unblock,
            ChatUserActionModel.Leave,
            ChatUserActionModel.AcceptRequest,
            ChatUserActionModel.DeclineRequest,
        )) {
            coVerify { productRoomUserActions.contactAction(roomId, action) }
        }
    }

    @Test
    fun `the input of a room of the chat product follows its contact`() = runBlocking<Unit> {
        every { roomPolicyProvider.observePolicy(roomId) } returns
            flowOf(contactRoom.copy(contact = contact.copy(state = ChatContactStateModel.INCOMING)))

        val input = interactor().subscribeUserInputState(OpenChatRequest.ExistingChat(roomId)).first()

        assertEquals(ChatUserInputState.AcceptChatRequest, input)
    }

    @Test
    fun `a sent welcome message turns the invite composer into the wait at once`() = runBlocking<Unit> {
        every { roomPolicyProvider.observePolicy(roomId) } returns flowOf(contactRoom.copy(contact = contact.copy(state = ChatContactStateModel.INVITE)))
        val waiting = mockk<ProductOutboxEntry> { every { isFailed } returns false }
        every { attachmentStager.observe(roomId) } returns flowOf(listOf(waiting))

        val input = interactor().subscribeUserInputState(OpenChatRequest.ExistingChat(roomId)).first()

        assertEquals(ChatUserInputState.WaitChatRequestApproval, input)
    }

    @Test
    fun `the edit history of a message comes from the product`() = runBlocking<Unit> {
        val querying = mockk<ProductChatQuerying> {
            coEvery { editHistory(roomId, "m1") } returns
                Result.success(listOf(ChatMessageRevisionModel(1, "first"), ChatMessageRevisionModel(2, null)))
        }
        coEvery { chatEngine.roomQuerying(roomId) } returns querying

        val history = interactor().getEditHistory(roomId, "m1")

        assertEquals(listOf("first"), history.map { it.text })
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
        callController = callController,
        callStateTracker = callStateTracker,
        fileUploadRepository = mockk(relaxed = true),
        fileUploadStarter = mockk(relaxed = true),
        attachmentMetaBuilder = mockk(relaxed = true),
        attachmentFileStorage = mockk(relaxed = true),
        hopNodeUrlProvider = mockk(relaxed = true),
        roomPolicyProvider = roomPolicyProvider,
        productRoomUserActions = productRoomUserActions,
        roomSourceContextFactory = roomSourceContextFactory,
        productAttachmentEncoder = productAttachmentEncoder,
        attachmentStager = attachmentStager,
        personChatRooms = personChatRooms,
        productPayments = FakeProductChatPaymentStore(),
    )
}
