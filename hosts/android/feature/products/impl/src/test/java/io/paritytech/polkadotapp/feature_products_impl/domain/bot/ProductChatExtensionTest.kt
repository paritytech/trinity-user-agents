package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import android.content.Context
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtensionContext
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomPreviewData
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatCommandModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomUserEvent
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListeners
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageIds
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.RoomParticipation
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductRoomEvent
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerRefCounter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerReference
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.RecordingWorker
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.WorkerModalityApi
import io.paritytech.polkadotapp.feature_products_impl.presentation.bot.menu.ProductChatMenuRenderer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.isActive
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

private const val ROOM = "truapi-playground"

class ProductChatExtensionTest {
    private val product = Product(ProductId.fromStoredValue("truapi-playground.dot"), "TrUAPI Playground", icon = null)
    private val extensionId = product.id.toChatExtensionId()
    private val declarations: ChatRoomDeclarationsRepository = mockk()
    private val feedListeners: ProductFeedListeners = mockk()
    private val room = ChatId.forExtensionRoom(extensionId, ROOM)

    private fun extension(refCounter: ProductWorkerRefCounter) =
        ProductChatExtension(mockk<Context>(relaxed = true), product, refCounter, declarations, feedListeners)

    private fun placedByHost(refCounter: ProductWorkerRefCounter) =
        ProductChatExtension(mockk<Context>(relaxed = true), product, refCounter, declarations, feedListeners, ProductChatExtensionOrigin.PlacedByHost)

    private fun placedByHost(refCounter: ProductWorkerRefCounter) =
        ProductChatExtension(mockk<Context>(relaxed = true), product, refCounter, declarations, ProductChatExtensionOrigin.PlacedByHost)

    private class FakeRefCounter(
        private val booted: ProductWorker? = null,
        private val bootFailure: Throwable? = null,
    ) : ProductWorkerRefCounter {
        var chatSurface: ProductChatMessaging? = null

        override suspend fun acquire(productId: ProductId, label: String): ProductWorkerReference {
            return object : ProductWorkerReference {
                override suspend fun worker(): ProductWorker = booted ?: throw requireNotNull(bootFailure)
                override suspend fun enableModalityApi(api: WorkerModalityApi) {
                    chatSurface = (api as WorkerModalityApi.Chat).messaging
                }

                override fun release() = Unit
            }
        }

        override fun chatMessaging(productId: ProductId): ProductChatMessaging = FakeChatMessaging()
    }

    private fun chatExtensionContext(
        scope: CoroutineScope,
        ownRooms: Flow<List<ChatId>>,
        incoming: Flow<ChatMessage> = emptyFlow(),
        roomDeclarations: ChatRoomDeclarations? = null,
    ): ChatExtensionContext {
        val computationalScope = object : ComputationalScope, CoroutineScope by scope {}

        return mockk<ChatExtensionContext>(relaxed = true).also {
            every { it.scope } returns computationalScope
            every { it.subscribeNewMessages(any(), any()) } returns incoming
            every { it.subscribeOwnRooms() } returns ownRooms
            coEvery { it.roomDeclarations(any()) } returns roomDeclarations
        }
    }

    private fun TestScope.withChatHost(
        ownRooms: Flow<List<ChatId>> = emptyFlow(),
        incoming: Flow<ChatMessage> = emptyFlow(),
        roomDeclarations: ChatRoomDeclarations? = null,
        block: context(ChatExtensionContext) (CoroutineScope) -> Unit,
    ) {
        val hostScope = CoroutineScope(StandardTestDispatcher(testScheduler))
        try {
            with(chatExtensionContext(hostScope, ownRooms, incoming, roomDeclarations)) { block(hostScope) }
            advanceUntilIdle()
        } finally {
            hostScope.cancel()
        }
    }

    @Test
    fun `a user message in a named room reaches the worker with that room`() = runTest {
        val worker = RecordingWorker()
        val message = ChatMessage.new(
            chatId = ChatId.forExtensionRoom(extensionId, ROOM),
            content = ChatMessage.Content.Text("hello"),
            origin = ChatMessageOrigin.User,
        )

        withChatHost(incoming = flowOf(message)) {
            extension(FakeRefCounter(worker)).startGlobalWork()
        }

        assertEquals(listOf<Pair<ProductChatIdParameter?, String>>(ProductChatIdParameter(ROOM) to "hello"), worker.delivered)
    }

    @Test
    fun `a boot failure does not take the chat host down with it`() = runTest {
        val refCounter = FakeRefCounter(bootFailure = IllegalStateException("core runtime unavailable"))
        val extension = extension(refCounter)
        val hostScope = CoroutineScope(StandardTestDispatcher(testScheduler))

        try {
            with(chatExtensionContext(hostScope, emptyFlow(), emptyFlow())) { extension.startGlobalWork() }
            advanceUntilIdle()

            assertTrue("the throw must not cancel the chat host", hostScope.isActive)
        } finally {
            hostScope.cancel()
        }
    }

    @Test
    fun `disposing stops the inbound collector instead of leaking it`() = runTest {
        val worker = RecordingWorker()
        val incoming = MutableSharedFlow<ChatMessage>(extraBufferCapacity = 4)
        val extension = extension(FakeRefCounter(worker))
        val hostScope = CoroutineScope(StandardTestDispatcher(testScheduler))

        try {
            with(chatExtensionContext(hostScope, emptyFlow(), incoming)) { extension.startGlobalWork() }
            advanceUntilIdle()
            incoming.tryEmit(textMessage("before"))
            advanceUntilIdle()
            assertEquals(1, incoming.subscriptionCount.value)

            extension.dispose()
            advanceUntilIdle()

            assertEquals("the collector must not outlive the extension", 0, incoming.subscriptionCount.value)
            assertEquals(listOf("before"), worker.delivered.map { it.second })
        } finally {
            hostScope.cancel()
        }
    }

    private fun textMessage(text: String) = ChatMessage.new(
        chatId = ChatId.forExtensionRoom(extensionId, ROOM),
        content = ChatMessage.Content.Text(text),
        origin = ChatMessageOrigin.User,
    )

    @Test
    fun `a declared footer without an input hides the input of that room alone`() = runTest {
        val extension = extension(FakeRefCounter(RecordingWorker()))
        val room = ChatId.forExtensionRoom(extensionId, ROOM)
        val other = ChatId.forExtensionRoom(extensionId, "other")
        every { declarations.observe(room) } returns flowOf(ChatRoomDeclarations.EMPTY.copy(footer = ChatRoomFooterModel.Empty))
        every { declarations.observe(other) } returns flowOf(null)

        assertEquals(false, extension.observeUserInputAllowed(room).first())
        assertEquals(true, extension.observeUserInputAllowed(other).first())
    }

    @Test
    fun `a footer the product draws hides the input until the host draws it`() = runTest {
        val extension = extension(FakeRefCounter(RecordingWorker()))
        val room = ChatId.forExtensionRoom(extensionId, ROOM)
        every { declarations.observe(room) } returns flowOf(ChatRoomDeclarations.EMPTY.copy(footer = ChatRoomFooterModel.Custom))

        assertEquals(false, extension.observeUserInputAllowed(room).first())
    }

    @Test
    fun `the roomless default chat is not a product room`() = runTest {
        val refCounter = FakeRefCounter(RecordingWorker())
        val ownRooms = flowOf(listOf(ChatId.forExtension(extensionId), ChatId.forExtensionRoom(extensionId, ROOM)))

        withChatHost(ownRooms) {
            extension(refCounter).startGlobalWork()
        }

        val rooms = requireNotNull(refCounter.chatSurface).subscribeChatRooms().first()
        assertEquals(listOf(ROOM to RoomParticipation.ROOM_HOST), rooms.map { it.roomId to it.participatingAs })
    }

    @Test
    fun `a user message reaches the product as a posted message with its host id`() = runTest {
        val worker = RecordingWorker()
        val message = textMessage("hello")

        withChatHost(incoming = flowOf(message)) { extension(FakeRefCounter(worker)).startGlobalWork() }

        assertEquals(
            listOf(ProductChatIdParameter(ROOM) to ProductRoomEvent.MessagePosted(message.id, message.content, null)),
            worker.roomEvents,
        )
    }

    @Test
    fun `a room whose product keeps the messages sends no user event`() = runTest {
        val worker = RecordingWorker()
        val productSource = ChatRoomDeclarations.EMPTY.copy(config = ChatRoomConfigModel.HOST_DEFAULT.copy(productSource = true))

        withChatHost(incoming = flowOf(textMessage("hello")), roomDeclarations = productSource) {
            extension(FakeRefCounter(worker)).startGlobalWork()
        }

        assertTrue(worker.roomEvents.isEmpty())
    }

    @Test
    fun `an edit and a reaction of the user name the product id of their message`() = runTest {
        val worker = RecordingWorker()
        val target = ProductMessageIds.stored("answer-1", room)
        val edit = userMessage(ChatMessage.Content.Edited(target, ChatMessage.Content.RichText("better", emptyList())))
        val reaction = userMessage(ChatMessage.Content.Reacted(target, ChatMessageReactionContent("👍")))

        withChatHost(incoming = flowOf(edit, reaction)) { extension(FakeRefCounter(worker)).startGlobalWork() }

        val events = worker.roomEvents.map { it.second }
        assertEquals("answer-1", (events[0] as ProductRoomEvent.UserEdited).messageId)
        assertEquals(ProductRoomEvent.UserReacted("answer-1", "👍", placed = true), events[1])
    }

    @Test
    fun `seen goes to the product once per message and only when the room asks for it`() = runTest {
        val worker = RecordingWorker()
        val extension = extension(FakeRefCounter(worker))
        withChatHost { extension.startGlobalWork() }
        withDeclarations(ChatRoomConfigModel.HOST_DEFAULT.copy(reportSeen = true))

        extension.onUserEvent(room, ChatRoomUserEvent.MessageSeen(ProductMessageIds.stored("m1", room)))
        extension.onUserEvent(room, ChatRoomUserEvent.MessageSeen(ProductMessageIds.stored("m1", room)))
        withDeclarations(ChatRoomConfigModel.HOST_DEFAULT)
        extension.onUserEvent(room, ChatRoomUserEvent.MessageSeen("m2"))

        assertEquals(listOf<ProductRoomEvent>(ProductRoomEvent.Seen("m1")), worker.roomEvents.map { it.second })
    }

    @Test
    fun `an action press goes to the product in either store mode`() = runTest {
        val worker = RecordingWorker()
        val extension = extension(FakeRefCounter(worker))
        withChatHost { extension.startGlobalWork() }
        withDeclarations(ChatRoomConfigModel.HOST_DEFAULT.copy(productSource = true))

        extension.onUserEvent(room, ChatRoomUserEvent.ActionTriggered(ProductMessageIds.stored("m1", room), "yes"))
        extension.onUserEvent(room, ChatRoomUserEvent.MessageDeleted("m1"))

        assertEquals(listOf<ProductRoomEvent>(ProductRoomEvent.ActionTriggered("m1", "yes")), worker.roomEvents.map { it.second })
    }

    @Test
    fun `only a declared command goes to the product`() = runTest {
        val worker = RecordingWorker()
        val extension = extension(FakeRefCounter(worker))
        withChatHost { extension.startGlobalWork() }
        coEvery { declarations.get(room) } returns ChatRoomDeclarations.EMPTY.copy(commands = listOf(ChatCommandModel("roll", "Roll", null)))

        val declared = extension.handleCommand(room, "roll", "2")
        val undeclared = extension.handleCommand(room, "help", "")

        assertTrue(declared)
        assertFalse(undeclared)
        assertEquals(listOf<ProductRoomEvent>(ProductRoomEvent.Command("roll", "2")), worker.roomEvents.map { it.second })
    }

    @Test
    fun `a live feed update goes to the screens of its room`() = runTest {
        val refCounter = FakeRefCounter(RecordingWorker())
        withChatHost { extension(refCounter).startGlobalWork() }
        val update = ProductFeedUpdateModel.Deleted("m1")
        coEvery { feedListeners.deliver(room, update) } returns true

        assertTrue(requireNotNull(refCounter.chatSurface).deliverLiveFeedUpdate(ProductChatIdParameter(ROOM), update))
    }

    private fun withDeclarations(config: ChatRoomConfigModel) {
        coEvery { declarations.get(room) } returns ChatRoomDeclarations.EMPTY.copy(config = config)
    }

    private fun userMessage(content: ChatMessage.Content) = ChatMessage.new(chatId = room, content = content, origin = ChatMessageOrigin.User)

    @Test
    fun `a product without a room runs a short while to create one`() = runTest {
        val worker = RecordingWorker()

        withChatHost(ownRooms = flowOf(emptyList())) { extension(FakeRefCounter(worker)).startGlobalWork() }

        assertEquals(1, worker.briefHolds)
    }

    @Test
    fun `a product with a room waits for a screen to open it`() = runTest {
        val worker = RecordingWorker()

        withChatHost(ownRooms = flowOf(listOf(room))) { extension(FakeRefCounter(worker)).startGlobalWork() }

        assertEquals(0, worker.briefHolds)
    }

    @Test
    fun `a room screen opens and closes the room of the product`() = runTest {
        val worker = RecordingWorker()
        val extension = extension(FakeRefCounter(worker))
        withChatHost(ownRooms = flowOf(listOf(room))) { extension.startGlobalWork() }

        val lifecycle = requireNotNull(extension.roomLifecycle(room))
        lifecycle.open(room)
        lifecycle.close(room)

        assertEquals(listOf("open" to ProductChatIdParameter(ROOM), "close" to ProductChatIdParameter(ROOM)), worker.roomCalls)
        assertEquals(null, extension.roomLifecycle(ChatId.forExtension(extensionId)))

    private suspend fun ProductChatExtension.pinnedAndRemovable(chatId: ChatId): Pair<ChatPreview?, Boolean> = coroutineScope {
        val computationalScope = object : ComputationalScope, CoroutineScope by this {}
        val preview = with(computationalScope) { customChatPreviewDelegate(chatId).provider.provide().first() }
        preview to (customMenuRenderer(chatId) as ProductChatMenuRenderer).removable
    }

    // The host put it there, so it holds the top of the list rather than competing on recency,
    // and nobody can remove it: a removal would only hide the row until the next launch while
    // its worker kept running.
    @Test
    fun `a host-placed chat is pinned above the rest and offers no removal`() = runTest {
        val extension = placedByHost(FakeRefCounter(RecordingWorker()))

        assertEquals(
            ChatPreview.Custom(Order.PinToTop, CustomPreviewData.FromMessage, ChatPreview.Custom.BadgeStyle.UNREAD) to false,
            extension.pinnedAndRemovable(ChatId.forExtensionRoom(extensionId, "jollity")),
        )
    }

    @Test
    fun `an ordinary product's chat competes on recency and can be removed`() = runTest {
        val extension = extension(FakeRefCounter(RecordingWorker()))

        assertEquals(null to true, extension.pinnedAndRemovable(ChatId.forExtensionRoom(extensionId, ROOM)))
    }

    @Test
    fun `a user action names its message by the product id`() = runTest {
        val worker = RecordingWorker()
        val extension = extension(FakeRefCounter(worker))
        withChatHost(ownRooms = flowOf(listOf(room))) { extension.startGlobalWork() }

        requireNotNull(extension.roomQuerying(room)).userAction(room, ChatUserActionModel.Retry(ProductMessageIds.stored("m1", room)))
        requireNotNull(extension.roomQuerying(room)).userAction(room, ChatUserActionModel.DeleteMessage("m2"))

        assertEquals(listOf<ChatUserActionModel>(ChatUserActionModel.Retry("m1"), ChatUserActionModel.DeleteMessage("m2")), worker.userActions)
    }

    @Test
    fun `a message menu question names the message and its reply by the product ids`() = runTest {
        val worker = RecordingWorker()
        val extension = extension(FakeRefCounter(worker))
        withChatHost(ownRooms = flowOf(listOf(room))) { extension.startGlobalWork() }
        val message = ProductRoomMessage(
            messageId = ProductMessageIds.stored("m1", room),
            participantId = "product",
            timestamp = 1L,
            content = ChatMessage.Content.Text("hi"),
            status = ProductMessageStatusModel.SENT,
            replyTo = ProductMessageIds.stored("m0", room),
            editedAt = null,
            reactions = emptyList(),
        )

        val menu = requireNotNull(extension.roomQuerying(room)).messageMenu(room, message)

        assertEquals(RecordingWorker.MENU, menu.getOrNull())
        assertEquals(listOf(message.copy(messageId = "m1", replyTo = "m0")), worker.menuQuestions)
    }
}
