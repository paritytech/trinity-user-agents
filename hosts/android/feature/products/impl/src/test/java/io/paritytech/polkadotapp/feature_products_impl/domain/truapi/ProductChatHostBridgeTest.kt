package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.parity.truapi.HOST_DEFAULT_CHAT_ROOM_CONFIG
import io.parity.truapi.HOST_DEFAULT_CHAT_ROOM_FEATURES
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomStatus
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatContactStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatPreviewReactionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallsModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomContactModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.UnknownChatRoomException
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.RoomParticipation
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.FakeChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductBotMessage
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomResult
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatRoom
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.HostCallException
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ChatAction
import uniffi.truapi.ChatActionLayout
import uniffi.truapi.ChatActions
import uniffi.truapi.ChatContactState
import uniffi.truapi.ChatCustomMessage
import uniffi.truapi.ChatFile
import uniffi.truapi.ChatMedia
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatPreviewReaction
import uniffi.truapi.ChatReaction
import uniffi.truapi.ChatRichText
import uniffi.truapi.ChatRoomBadge
import uniffi.truapi.ChatRoomCall
import uniffi.truapi.ChatRoomCalls
import uniffi.truapi.ChatRoomContact
import uniffi.truapi.ChatRoomEvent
import uniffi.truapi.ChatRoomFeatures
import uniffi.truapi.ChatRoomFooter
import uniffi.truapi.ChatRoomMetadata
import uniffi.truapi.ChatRoomParticipation
import uniffi.truapi.ChatRoomPreview
import uniffi.truapi.ChatRoomRegistrationStatus
import uniffi.truapi.HostRejection

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
        val created = runCatching { bridge.createRoom("", "Jollity", "icon") }.exceptionOrNull()
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
    fun `registerBot is refused, as this host keeps no bots`() = runTest {
        val thrown = runCatching { bridge(FakeChatMessaging()).registerBot("helper", "Helper", "") }.exceptionOrNull()

        assertTrue(thrown is HostRejection)
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

        val thrown = runCatching { bridge(api).createRoom(ROOM, "Jollity", "icon") }.exceptionOrNull()

        assertTrue(thrown is HostRejection)
        assertTrue(thrown!!.message!!.contains("Product messaging is not supported"))
        assertFalse(thrown.message!!.contains("may still"))
    }

    @Test
    fun `createRoom maps New and Exists to the core registration status`() = runTest {
        suspend fun statusFor(status: CreateRoomStatus) = bridge(
            FakeChatMessaging(onCreateRoom = { Result.success(CreateProductRoomResult(status)) }),
        ).createRoom(ROOM, "Jollity", "icon")

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
    fun `a body this host cannot render is rejected by name, as before the chat product`() = runTest {
        val api = FakeChatMessaging(onSendMessage = { _, _ -> Result.success("message-id") })
        val bodies = listOf(
            ChatMessageContent.RichText(ChatRichText("Look", listOf(ChatMedia("data:image/png;base64,AA==")))),
            ChatMessageContent.Actions(ChatActions("Pick", listOf(ChatAction("yes", "Yes")), ChatActionLayout.GRID)),
            ChatMessageContent.File(ChatFile("https://example.com/a.pdf", "a.pdf", "application/pdf", 12uL, "Here")),
            ChatMessageContent.Reaction(ChatReaction("m1", "👍")),
            ChatMessageContent.ReactionRemoved(ChatReaction("m1", "👍")),
        )

        for (body in bodies) {
            val thrown = runCatching { bridge(api).postMessage(ROOM, body, alt = null) }.exceptionOrNull()

            assertTrue(body.javaClass.simpleName, thrown is HostRejection)
            assertTrue(thrown!!.message!!.contains(body.javaClass.simpleName))
        }
        assertTrue(api.sentText.isEmpty())
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
        coVerify {
            messaging.setRoomMetadata(
                room,
                "Lobby",
                ChatRoomMetadataModel("Play here"),
            )
        }
        coVerify {
            messaging.setRoomPreview(
                room,
                ChatRoomPreviewModel(5, ChatRoomBadgeModel.Count(2), null, ChatPreviewReactionModel("👍", byUser = false, message = null, removed = true), 3),
            )
        }
        coVerify { messaging.deleteRoom(room) }
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

    @Test
    fun `capabilities lists only the features that this build serves from end to end`() = runTest {
        assertEquals(
            listOf("product_source_rooms", "hop_attachments", "call_buttons", "contact_rooms"),
            bridge(FakeChatMessaging()).capabilities(),
        )
    }

    @Test
    fun `a room without declared features answers the host default features`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        coEvery { messaging.roomDeclarations(ProductChatIdParameter(ROOM)) } returns Result.success(ChatRoomDeclarations.EMPTY)

        assertEquals(HOST_DEFAULT_CHAT_ROOM_FEATURES, ProductChatHostBridge(productId, messaging).roomFeatures(ROOM))
    }

    @Test
    fun `declared features are answered as declared`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        val declared = ChatRoomFeaturesModel(calls = ChatRoomCallsModel(audio = true, video = false), hopAttachments = true)
        coEvery { messaging.roomDeclarations(ProductChatIdParameter(ROOM)) } returns
            Result.success(ChatRoomDeclarations.EMPTY.copy(features = declared))

        val features = ProductChatHostBridge(productId, messaging).roomFeatures(ROOM)

        assertEquals(ChatRoomFeatures(ChatRoomCalls(true, false), hopAttachments = true), features)
    }

    @Test
    fun `features and the call reach the chat binding as models`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        val room = ProductChatIdParameter(ROOM)
        coEvery { messaging.setRoomFeatures(any(), any()) } returns Result.success(Unit)
        coEvery { messaging.setRoomCall(any(), any()) } returns Result.success(Unit)
        val bridge = ProductChatHostBridge(productId, messaging)

        bridge.setRoomFeatures(ROOM, HOST_DEFAULT_CHAT_ROOM_FEATURES.copy(hopAttachments = true))
        bridge.setRoomCall(ROOM, ChatRoomCall.Ringing("c1", video = true, caller = "p1"))
        bridge.setRoomCall(ROOM, null)

        coVerify { messaging.setRoomFeatures(room, ChatRoomFeaturesModel.HOST_DEFAULT.copy(hopAttachments = true)) }
        coVerify { messaging.setRoomCall(room, ChatRoomCallModel.Ringing("c1", video = true, caller = "p1")) }
        coVerify { messaging.setRoomCall(room, null) }
    }

    @Test
    fun `the contact of a room reaches the chat binding as a model`() = runTest {
        val api = FakeChatMessaging()

        bridge(api).setRoomContact(ROOM, ChatRoomContact(key(), "alice", ChatContactState.INCOMING))

        val (room, contact) = api.contacts.single()
        assertEquals(ProductChatIdParameter(ROOM), room)
        assertEquals(ChatRoomContactModel(key().toDataByteArray(), "alice", ChatContactStateModel.INCOMING), contact)
    }

    @Test
    fun `the username of a contact has the native two-digit index`() = runTest {
        val api = FakeChatMessaging()

        bridge(api).setRoomContact(ROOM, ChatRoomContact(key(), "alice.7", ChatContactState.ESTABLISHED))
        bridge(api).setRoomContact(ROOM, ChatRoomContact(key(), "bob", ChatContactState.ESTABLISHED))
        bridge(api).setRoomContact(ROOM, ChatRoomContact(key(), "carol.dot", ChatContactState.ESTABLISHED))

        assertEquals(listOf("alice.07", "bob", "carol.dot"), api.contacts.map { it.second.username })
    }

    @Test
    fun `a call request carries the call id that the host chose`() {
        assertEquals(
            ChatRoomEvent.CallRequested(video = true, callId = "c-1"),
            ChatRoomEventModel.CallRequested(video = true, callId = "c-1").toCore(),
        )
    }

    private fun key() = ByteArray(32) { 5 }

    private fun bridge(api: FakeChatMessaging) = ProductChatHostBridge(productId, api)
}

private fun <T> messagingNotSupported(): Result<T> = Result.failure(
    HostCallException(MESSAGING_NOT_SUPPORTED_CODE, "Product messaging is not supported in this context"),
)
