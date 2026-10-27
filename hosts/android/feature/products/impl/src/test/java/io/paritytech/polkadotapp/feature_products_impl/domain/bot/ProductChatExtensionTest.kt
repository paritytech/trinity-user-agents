package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import android.content.Context
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtensionContext
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomPreviewData
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
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
import io.paritytech.polkadotapp.feature_products_impl.domain.product.productIconUri
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

    private val feedListeners: ProductFeedListeners = mockk()
    private val room = ChatId.forExtensionRoom(extensionId, ROOM)

    private fun extension(refCounter: ProductWorkerRefCounter) =
        ProductChatExtension(mockk<Context>(relaxed = true), product, refCounter, mockk(), mockk(), feedListeners)

    private fun placedByHost(refCounter: ProductWorkerRefCounter) = ProductChatExtension(
        mockk<Context>(relaxed = true),
        product,
        refCounter,
        mockk(),
        mockk(),
        feedListeners,
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
    fun `a live feed update goes to the screens of its room`() = runTest {
        val refCounter = FakeRefCounter(RecordingWorker())
        withChatHost { extension(refCounter).startGlobalWork() }
        val update = ProductFeedUpdateModel.StatusChanged("m1", ProductMessageStatusModel.READ)
        coEvery { feedListeners.deliver(room, update) } returns true

        assertTrue(requireNotNull(refCounter.chatSurface).deliverLiveFeedUpdate(ProductChatIdParameter(ROOM), update))
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
    fun `a room whose product keeps the messages sends no user message`() = runTest {
        val worker = RecordingWorker()
        val productSource = ChatRoomDeclarations.EMPTY.copy(config = ChatRoomConfigModel.HOST_DEFAULT.copy(productSource = true))

        withChatHost(incoming = flowOf(textMessage("hello")), roomDeclarations = productSource) {
            extension(FakeRefCounter(worker)).startGlobalWork()
        }

        assertTrue(worker.delivered.isEmpty())
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

    // Until the worker registers an icon for its room, the chat shows the one the manifest declares.
    @Test
    fun `a product's chat defaults to its manifest icon`() {
        val extension = extension(FakeRefCounter(RecordingWorker()))

        assertEquals(product.id.productIconUri(), extension.defaultRoomMetadata.icon)
    }
}
