package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import android.content.Context
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtensionContext
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomPreviewData
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatContactStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomContactModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListeners
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageIds
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.RoomParticipation
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductChatRoomEventSink
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
import org.junit.Assert.assertTrue
import org.junit.Test

private const val ROOM = "truapi-playground"

class ProductChatExtensionTest {
    private val product = Product(ProductId.fromStoredValue("truapi-playground.dot"), "TrUAPI Playground", icon = null)
    private val extensionId = product.id.toChatExtensionId()
    private val declarations: ChatRoomDeclarationsRepository = mockk {
        every { observe(any()) } returns flowOf(null)
    }
    private val feedListeners: ProductFeedListeners = mockk()
    private val roomEventSink: ProductChatRoomEventSink = mockk()
    private val calls: ChatRoomCallRepository = mockk()
    private val room = ChatId.forExtensionRoom(extensionId, ROOM)

    private fun extension(refCounter: ProductWorkerRefCounter) =
        ProductChatExtension(mockk<Context>(relaxed = true), product, refCounter, mockk(), declarations, feedListeners, roomEventSink, calls)

    private fun placedByHost(refCounter: ProductWorkerRefCounter) =
        ProductChatExtension(
            mockk<Context>(relaxed = true),
            product,
            refCounter,
            mockk(),
            declarations,
            feedListeners,
            roomEventSink,
            calls,
            ProductChatExtensionOrigin.PlacedByHost,
        )

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

    @Test
    fun `a stopped worker clears the ringing calls of the product and keeps an active call`() = runTest {
        val refCounter = FakeRefCounter(RecordingWorker())
        val ringing = ChatId.forExtensionRoom(extensionId, ROOM)
        val active = ChatId.forExtensionRoom(extensionId, "active")
        every { calls.observe(ringing) } returns flowOf(ChatRoomCallModel.Ringing("c1", video = false, caller = "p1"))
        every { calls.observe(active) } returns flowOf(ChatRoomCallModel.Active("c2", video = true, startedAt = 1))
        val hostScope = CoroutineScope(StandardTestDispatcher(testScheduler))

        try {
            val host = chatExtensionContext(hostScope, flowOf(listOf(ringing, active)))
            coEvery { host.setRoomCall(any(), any()) } returns Result.success(Unit)
            with(host) { extension(refCounter).startGlobalWork() }
            advanceUntilIdle()

            assertTrue(requireNotNull(refCounter.chatSurface).clearRingingCalls().isSuccess)

            coVerify(exactly = 1) { host.setRoomCall(ringing, null) }
            coVerify(exactly = 0) { host.setRoomCall(active, any()) }
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
    fun `an empty footer hides the input of that room alone`() = runTest {
        val refCounter = FakeRefCounter(RecordingWorker())
        val extension = extension(refCounter)

        withChatHost { extension.startGlobalWork() }
        val messaging = requireNotNull(refCounter.chatSurface)

        assertEquals(true, extension.observeUserInputAllowed(ChatId.forExtensionRoom(extensionId, ROOM)).first())

        messaging.setRoomFooter(ProductChatIdParameter(ROOM), showsTextInput = false)

        assertEquals(false, extension.observeUserInputAllowed(ChatId.forExtensionRoom(extensionId, ROOM)).first())
        assertEquals(true, extension.observeUserInputAllowed(ChatId.forExtensionRoom(extensionId, "other")).first())
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
    fun `a text of the user reaches the product as a user message`() = runTest {
        val worker = RecordingWorker()

        withChatHost(incoming = flowOf(textMessage("hello"))) { extension(FakeRefCounter(worker)).startGlobalWork() }

        assertEquals(listOf(ProductChatIdParameter(ROOM) to "hello"), worker.delivered)
    }

    @Test
    fun `a room whose product keeps the messages sends no user message`() = runTest {
        val worker = RecordingWorker()
        val productSource = ChatRoomDeclarations.EMPTY.copy(config = ChatRoomConfigModel.HOST_DEFAULT.copy(productSource = true))

        withChatHost(incoming = flowOf(textMessage("hello")), roomDeclarations = productSource) {
            extension(FakeRefCounter(worker)).startGlobalWork()
        }

        assertTrue(worker.delivered.isEmpty())
    }

    @Test
    fun `a room event of the additions goes to the core, not through the worker`() = runTest {
        val worker = RecordingWorker()
        val extension = extension(FakeRefCounter(worker))
        withChatHost { extension.startGlobalWork() }
        val event = ChatRoomEventModel.CallRequested(video = true)
        coEvery { roomEventSink.publish(any(), any(), any()) } returns Result.success(Unit)

        extension.onRoomEvent(room, event)

        coVerify { roomEventSink.publish(product.id, ProductChatIdParameter(ROOM), event) }
        assertTrue(worker.delivered.isEmpty())
    }

    @Test
    fun `a room event of another extension is not sent`() = runTest {
        extension(FakeRefCounter(RecordingWorker())).onRoomEvent(ChatId.forExtensionRoom("ProductBot_other.dot", ROOM), ChatRoomEventModel.CallEnded("c1"))

        coVerify(exactly = 0) { roomEventSink.publish(any(), any(), any()) }
    }

    @Test
    fun `a live feed update goes to the screens of its room`() = runTest {
        val refCounter = FakeRefCounter(RecordingWorker())
        withChatHost { extension(refCounter).startGlobalWork() }
        val update = ProductFeedUpdateModel.StatusChanged("m1", ProductMessageStatusModel.READ)
        coEvery { feedListeners.deliver(room, update) } returns true

        assertTrue(requireNotNull(refCounter.chatSurface).deliverLiveFeedUpdate(ProductChatIdParameter(ROOM), update))
    }

    private fun withDeclarations(config: ChatRoomConfigModel) {
        coEvery { declarations.get(room) } returns ChatRoomDeclarations.EMPTY.copy(config = config)
    }

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
    }

    @Test
    fun `a user action names its message by the product id`() = runTest {
        val worker = RecordingWorker()
        val extension = extension(FakeRefCounter(worker))
        withChatHost(ownRooms = flowOf(listOf(room))) { extension.startGlobalWork() }

        requireNotNull(extension.roomQuerying(room)).userAction(room, ChatUserActionModel.Seen(ProductMessageIds.stored("m1", room)))
        requireNotNull(extension.roomQuerying(room)).userAction(room, ChatUserActionModel.Seen("m2"))

        assertEquals(listOf<ChatUserActionModel>(ChatUserActionModel.Seen("m1"), ChatUserActionModel.Seen("m2")), worker.userActions)
    }

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
    fun `a person chat of a host-placed chat product is ordered by time, as a native person chat`() = runTest {
        val extension = placedByHost(FakeRefCounter(RecordingWorker()))
        val personChat = ChatId.forExtensionRoom(extensionId, "dm-b0")
        val contact = ChatRoomContactModel(DataByteArray(ByteArray(32)), "bob.01", ChatContactStateModel.ESTABLISHED)
        every { declarations.observe(personChat) } returns flowOf(ChatRoomDeclarations.EMPTY.copy(contact = contact))

        assertEquals(null, extension.pinnedAndRemovable(personChat).first)
    }

    @Test
    fun `an ordinary product's chat competes on recency and can be removed`() = runTest {
        val extension = extension(FakeRefCounter(RecordingWorker()))

        assertEquals(null to true, extension.pinnedAndRemovable(ChatId.forExtensionRoom(extensionId, ROOM)))
    }
}
