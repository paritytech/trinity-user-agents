package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.parity.truapi.HOST_DEFAULT_CHAT_ROOM_CONFIG
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomStatus
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatPreviewReactionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.UnknownChatRoomException
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.RoomParticipation
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.FakeChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductBotMessage
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomResult
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageContent
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatRoom
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.HostCallException
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ChatCustomMessage
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageOrigin
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatPreviewReaction
import uniffi.truapi.ChatReaction
import uniffi.truapi.ChatRoomBadge
import uniffi.truapi.ChatRoomFooter
import uniffi.truapi.ChatRoomMetadata
import uniffi.truapi.ChatRoomParticipation
import uniffi.truapi.ChatRoomPreview
import uniffi.truapi.ChatRoomRegistrationStatus
import uniffi.truapi.HostRejection
import uniffi.truapi.ChatMessage as CoreChatMessage

private const val MESSAGING_NOT_SUPPORTED_CODE = "messaging_not_supported"
private const val ROOM = "jollity"

class ProductChatHostBridgeTest {
    private val productId = ProductId.fromStoredValue("dim2.dot")

    @Test
    fun `a text message is posted to the room the core names`() = runTest {
        val api = FakeChatMessaging(onSendMessage = { _, _ -> Result.success("message-id") })

        bridge(api).postMessage(ROOM, ChatMessageContent.Text("hi"), alt = null)

        assertEquals(listOf(ROOM to "hi"), api.sentText)
    }

    @Test
    fun `a custom message carries the product's alt to the host`() = runTest {
        var sent: ProductBotMessage? = null
        val api = FakeChatMessaging(onSendMessage = { _, message -> sent = message; Result.success("message-id") })

        val content = ChatMessageContent.Custom(ChatCustomMessage(messageType = "weekly-draw", payload = byteArrayOf(1)))
        bridge(api).postMessage(ROOM, content, alt = "Weekly draw")

        assertEquals(ProductBotMessage.Custom("weekly-draw", byteArrayOf(1).toDataByteArray(), alt = "Weekly draw"), sent)
    }

    @Test
    fun `an empty room id is rejected by every room-scoped call`() = runTest {
        val api = FakeChatMessaging(onSendMessage = { _, _ -> Result.success("message-id") })
        val bridge = bridge(api)

        val posted = runCatching { bridge.postMessage("", ChatMessageContent.Text("hi"), alt = null) }.exceptionOrNull()
        val created = runCatching { bridge.createRoom("", "Jollity", "") }.exceptionOrNull()
        val footed = runCatching { bridge.setRoomFooter("", ChatRoomFooter.EMPTY) }.exceptionOrNull()

        assertTrue(posted is HostRejection)
        assertTrue(posted!!.message!!.contains("a chat message needs a room"))
        assertTrue(created is HostRejection)
        assertTrue(created!!.message!!.contains("a chat room needs an id"))
        assertTrue(footed is HostRejection)
        assertTrue(footed!!.message!!.contains("a chat room needs an id"))
        assertTrue(api.sentText.isEmpty())
        assertTrue(api.footers.isEmpty())
    }

    @Test
    fun `an unsupported content variant is rejected by name`() = runTest {
        val api = FakeChatMessaging()

        val thrown = runCatching {
            bridge(api).postMessage(ROOM, ChatMessageContent.Reaction(reaction()), alt = null)
        }.exceptionOrNull()

        assertTrue(thrown is HostRejection)
        assertTrue(thrown!!.message!!.contains("Reaction"))
        assertTrue(api.sentText.isEmpty())
    }

    @Test
    fun `registerBot is rejected — this host has no bot registry`() = runTest {
        val thrown = runCatching { bridge(FakeChatMessaging()).registerBot("b", "B", "") }.exceptionOrNull()

        assertTrue(thrown is HostRejection)
        assertTrue(thrown!!.message!!.contains("bot registry"))
    }

    @Test
    fun `an unbound surface lists no rooms`() = runTest {
        assertEquals(emptyList<Any>(), bridge(FakeChatMessaging()).listRooms())
    }

    @Test
    fun `listRooms maps the RoomHost and Bot participation strings to the core enum`() = runTest {
        val rooms = listOf(ProductChatRoom("room-1", RoomParticipation.ROOM_HOST), ProductChatRoom("room-2", RoomParticipation.BOT))

        assertEquals(
            listOf(
                "room-1" to ChatRoomParticipation.ROOM_HOST,
                "room-2" to ChatRoomParticipation.BOT,
            ),
            bridge(FakeChatMessaging(rooms = flowOf(rooms))).listRooms().map { it.roomId to it.participatingAs },
        )
    }

    @Test
    fun `a failing createRoom surfaces as a rejection carrying its reason`() = runTest {
        val api = FakeChatMessaging(onCreateRoom = { messagingNotSupported() })

        val thrown = runCatching { bridge(api).createRoom(ROOM, "Jollity", "") }.exceptionOrNull()

        assertTrue(thrown is HostRejection)
        assertTrue(thrown!!.message!!.contains("Product messaging is not supported"))
        assertFalse(thrown.message!!.contains("may still"))
    }

    // The core sends an absent icon as an empty string, which must not hide the room's manifest icon.
    @Test
    fun `a room registered without an icon keeps its default icon`() = runTest {
        var registered: CreateProductRoomRequest? = null
        val api = FakeChatMessaging(onCreateRoom = { registered = it; Result.success(CreateProductRoomResult(CreateRoomStatus.New)) })

        bridge(api).createRoom(ROOM, "Jollity", icon = "")

        assertEquals(null, registered?.icon)
    }

    // The room keeps the icon in the form the core screens it to, so the bridge must build it through the core.
    @Test
    fun `a room registers the icon in the form the core screens it to`() = runTest {
        var registered: CreateProductRoomRequest? = null
        val api = FakeChatMessaging(onCreateRoom = { registered = it; Result.success(CreateProductRoomResult(CreateRoomStatus.New)) })

        bridge(api).createRoom(ROOM, "Jollity", icon = "https://example.invalid")

        assertEquals("https://example.invalid/", registered?.icon?.value)
    }

    @Test
    fun `createRoom maps New and Exists to the core registration status`() = runTest {
        suspend fun statusFor(status: CreateRoomStatus) = bridge(
            FakeChatMessaging(onCreateRoom = { Result.success(CreateProductRoomResult(status)) }),
        ).createRoom(ROOM, "Jollity", "")

        assertEquals(ChatRoomRegistrationStatus.NEW, statusFor(CreateRoomStatus.New))
        assertEquals(ChatRoomRegistrationStatus.EXISTS, statusFor(CreateRoomStatus.Exists))
    }

    @Test
    fun `setRoomFooter maps the core footer onto the room's text input`() = runTest {
        val api = FakeChatMessaging()
        val bridge = bridge(api)

        bridge.setRoomFooter(ROOM, ChatRoomFooter.EMPTY)
        bridge.setRoomFooter(ROOM, ChatRoomFooter.TEXT_INPUT)

        assertEquals(
            listOf(ProductChatIdParameter(ROOM) to false, ProductChatIdParameter(ROOM) to true),
            api.footers,
        )
    }

    @Test
    fun `a room without declarations answers the host default config`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        coEvery { messaging.roomDeclarations(ProductChatIdParameter(ROOM)) } returns Result.success(null)

        assertEquals(HOST_DEFAULT_CHAT_ROOM_CONFIG, ProductChatHostBridge(productId, messaging).roomConfig(ROOM))
    }

    @Test
    fun `a declared config is answered as declared`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        val declared = ChatRoomConfigModel.HOST_DEFAULT.copy(productSource = true, reportSeen = true)
        coEvery { messaging.roomDeclarations(ProductChatIdParameter(ROOM)) } returns
            Result.success(ChatRoomDeclarations.EMPTY.copy(config = declared))

        val config = ProductChatHostBridge(productId, messaging).roomConfig(ROOM)

        assertTrue(config.productSource)
        assertTrue(config.reportSeen)
    }

    @Test
    fun `each declaration reaches the chat binding as a model`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        val room = ProductChatIdParameter(ROOM)
        coEvery { messaging.setRoomConfig(any(), any()) } returns Result.success(Unit)
        coEvery { messaging.setRoomMetadata(any(), any(), any()) } returns Result.success(Unit)
        coEvery { messaging.setRoomPreview(any(), any()) } returns Result.success(Unit)
        coEvery { messaging.deleteRoom(any()) } returns Result.success(Unit)
        val bridge = ProductChatHostBridge(productId, messaging)

        bridge.setRoomConfig(ROOM, HOST_DEFAULT_CHAT_ROOM_CONFIG.copy(reportSeen = true))
        bridge.setRoomMetadata(ROOM, ChatRoomMetadata("Lobby", "Play here"))
        bridge.setRoomPreview(
            ROOM,
            ChatRoomPreview(5uL, ChatRoomBadge.UnreadCount(2u), null, ChatPreviewReaction("👍", byUser = false, message = null, removed = true), 3u),
        )
        bridge.deleteRoom(ROOM)

        coVerify { messaging.setRoomConfig(room, ChatRoomConfigModel.HOST_DEFAULT.copy(reportSeen = true)) }
        coVerify { messaging.setRoomMetadata(room, "Lobby", ChatRoomMetadataModel("Play here")) }
        coVerify {
            messaging.setRoomPreview(
                room,
                ChatRoomPreviewModel(5, ChatRoomBadgeModel.Count(2), null, ChatPreviewReactionModel("👍", byUser = false, message = null, removed = true), 3),
            )
        }
        coVerify { messaging.deleteRoom(room) }
    }

    // The chat list draws a custom message with the renderer of the product that sent it, as it draws a posted one.
    @Test
    fun `a custom last message is drawn by the renderer of the product`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        coEvery { messaging.setRoomPreview(any(), any()) } returns Result.success(Unit)
        val custom = CoreChatMessage(
            messageId = "m1",
            origin = ChatMessageOrigin("contact"),
            timestamp = 5uL,
            body = ChatMessageBody.Custom(ChatCustomMessage("poll", byteArrayOf(1, 2)), alt = "A poll"),
            status = ChatMessageStatus.DELIVERED,
            replyTo = null,
            replyPreview = null,
            editedAt = null,
            reactions = emptyList(),
            request = false,
        )

        ProductChatHostBridge(productId, messaging).setRoomPreview(ROOM, ChatRoomPreview(null, null, custom, null, 0u))

        val drawn = ChatMessage.Content.Custom(
            rendererId = productId.toChatExtensionId(),
            content = Result.success(ProductsMessageContent("poll", byteArrayOf(1, 2).toDataByteArray(), "A poll")),
        )
        coVerify { messaging.setRoomPreview(ProductChatIdParameter(ROOM), match { it.lastMessage?.content == drawn }) }
    }

    @Test
    fun `a declaration for an unknown room is rejected`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        coEvery { messaging.setRoomPreview(any(), any()) } returns Result.failure(UnknownChatRoomException())

        val thrown = runCatching {
            ProductChatHostBridge(productId, messaging).setRoomPreview(ROOM, ChatRoomPreview(null, null, null, null, 0u))
        }.exceptionOrNull()

        assertTrue(thrown is HostRejection)
        assertTrue(thrown!!.message!!.contains("no such chat room"))
    }

    private fun bridge(api: FakeChatMessaging) = ProductChatHostBridge(productId, api)

    private fun reaction() = ChatReaction(messageId = "msg-1", emoji = "👍")
}

private fun <T> messagingNotSupported(): Result<T> = Result.failure(
    HostCallException(MESSAGING_NOT_SUPPORTED_CODE, "Product messaging is not supported in this context"),
)
