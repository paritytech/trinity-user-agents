package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import android.content.Context
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtensionContext
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomResult
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomStatus
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomPreviewData
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.RoomParticipation
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
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

    private val placedRoom = ProductChatIdParameter("jollity")

    private fun hostPlaced(refCounter: ProductWorkerRefCounter) =
        ProductChatExtension(mockk<Context>(relaxed = true), product, refCounter, placedRoom = placedRoom)

    private class FakeRefCounter(
        private val booted: ProductWorker? = null,
        private val bootFailure: Throwable? = null,
        private val onAcquire: () -> Unit = {},
    ) : ProductWorkerRefCounter {
        var chatSurface: ProductChatMessaging? = null

        override suspend fun acquire(productId: ProductId, label: String): ProductWorkerReference {
            onAcquire()
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
    ): ChatExtensionContext {
        val computationalScope = object : ComputationalScope, CoroutineScope by scope {}

        return mockk<ChatExtensionContext>(relaxed = true).also {
            every { it.scope } returns computationalScope
            every { it.subscribeNewMessages(any(), any()) } returns incoming
            every { it.subscribeOwnRooms() } returns ownRooms
        }
    }

    private fun TestScope.withChatHost(
        ownRooms: Flow<List<ChatId>> = emptyFlow(),
        incoming: Flow<ChatMessage> = emptyFlow(),
        block: context(ChatExtensionContext) (CoroutineScope) -> Unit,
    ) {
        val hostScope = CoroutineScope(StandardTestDispatcher(testScheduler))
        try {
            with(chatExtensionContext(hostScope, ownRooms, incoming)) { block(hostScope) }
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
            ProductChatExtension(mockk<Context>(relaxed = true), product, FakeRefCounter(worker)).startGlobalWork()
        }

        assertEquals(listOf<Pair<ProductChatIdParameter?, String>>(ProductChatIdParameter(ROOM) to "hello"), worker.delivered)
    }

    @Test
    fun `a boot failure does not take the chat host down with it`() = runTest {
        val refCounter = FakeRefCounter(bootFailure = IllegalStateException("core runtime unavailable"))
        val extension = ProductChatExtension(mockk<Context>(relaxed = true), product, refCounter)
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
        val extension = ProductChatExtension(mockk<Context>(relaxed = true), product, FakeRefCounter(worker))
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
    fun `an empty footer hides the input of that room alone`() = runTest {
        val refCounter = FakeRefCounter(RecordingWorker())
        val extension = ProductChatExtension(mockk<Context>(relaxed = true), product, refCounter)

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
            ProductChatExtension(mockk<Context>(relaxed = true), product, refCounter).startGlobalWork()
        }

        val rooms = requireNotNull(refCounter.chatSurface).subscribeChatRooms().first()
        assertEquals(listOf(ROOM to RoomParticipation.ROOM_HOST), rooms.map { it.roomId to it.participatingAs })
    }

    // The row is the host's own: it has to be on the list before the worker has downloaded and
    // booted, which is the whole point of placing it. No metadata, so the list shows the product's
    // name until the product's own createRoom finds the row and supplies its own.
    @Test
    fun `a host-placed room is on the list before its worker is acquired`() = runTest {
        val events = mutableListOf<String>()
        val refCounter = FakeRefCounter(RecordingWorker(), onAcquire = { events += "worker" })
        val hostScope = CoroutineScope(StandardTestDispatcher(testScheduler))
        val context = chatExtensionContext(hostScope, emptyFlow())
        var placed: CreateRoomRequest? = null
        coEvery { context.createRoom(any()) } answers {
            placed = firstArg()
            events += "room"
            CreateRoomResult(firstArg<CreateRoomRequest>().chatId, CreateRoomStatus.New)
        }

        try {
            with(context) { hostPlaced(refCounter).startGlobalWork() }
            advanceUntilIdle()
        } finally {
            hostScope.cancel()
        }

        assertEquals(listOf("room", "worker"), events)
        val request = requireNotNull(placed)
        assertEquals(
            listOf(ChatId.forExtensionRoom(extensionId, "jollity"), null, null),
            listOf(request.chatId, request.name, request.icon),
        )
    }

    // A failure costs the placeholder, not the room: the product's own createRoom still runs, so
    // the worker has to come up regardless.
    @Test
    fun `a room that cannot be placed does not cost the worker`() = runTest {
        var acquired = false
        val refCounter = FakeRefCounter(RecordingWorker(), onAcquire = { acquired = true })
        val hostScope = CoroutineScope(StandardTestDispatcher(testScheduler))
        val context = chatExtensionContext(hostScope, emptyFlow())
        coEvery { context.createRoom(any()) } throws IllegalStateException("database locked")

        try {
            with(context) { hostPlaced(refCounter).startGlobalWork() }
            advanceUntilIdle()
        } finally {
            hostScope.cancel()
        }

        assertTrue(acquired)
    }

    @Test
    fun `an ordinary product places no room of its own`() = runTest {
        val hostScope = CoroutineScope(StandardTestDispatcher(testScheduler))
        val context = chatExtensionContext(hostScope, emptyFlow())

        try {
            with(context) {
                ProductChatExtension(mockk<Context>(relaxed = true), product, FakeRefCounter(RecordingWorker())).startGlobalWork()
            }
            advanceUntilIdle()
        } finally {
            hostScope.cancel()
        }

        coVerify(exactly = 0) { context.createRoom(any()) }
    }

    private suspend fun ProductChatExtension.pinnedAndRemovable(chatId: ChatId): Pair<ChatPreview?, Boolean> = coroutineScope {
        val computationalScope = object : ComputationalScope, CoroutineScope by this {}
        val preview = with(computationalScope) { customChatPreviewDelegate(chatId)?.provider?.provide()?.first() }
        preview to (customMenuRenderer(chatId) as ProductChatMenuRenderer).removable
    }

    // The host put it there, so it holds the top of the list rather than competing on recency,
    // and nobody can remove it: a removal would only hide the row until the next launch while
    // its worker kept running.
    @Test
    fun `a host-placed chat is pinned above the rest and offers no removal`() = runTest {
        val extension = hostPlaced(FakeRefCounter(RecordingWorker()))

        assertEquals(
            ChatPreview.Custom(Order.PinToTop, CustomPreviewData.FromMessage, ChatPreview.Custom.BadgeStyle.UNREAD) to false,
            extension.pinnedAndRemovable(ChatId.forExtensionRoom(extensionId, "jollity")),
        )
    }

    @Test
    fun `an ordinary product's chat competes on recency and can be removed`() = runTest {
        val extension = ProductChatExtension(mockk<Context>(relaxed = true), product, FakeRefCounter(RecordingWorker()))

        assertEquals(null to true, extension.pinnedAndRemovable(ChatId.forExtensionRoom(extensionId, ROOM)))
    }
}
