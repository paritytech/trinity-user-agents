package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.parity.truapi.HOST_DEFAULT_CHAT_ROOM_CONFIG
import io.parity.truapi.HOST_DEFAULT_CHAT_ROOM_FEATURES
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomStatus
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionButtonModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionHandlerModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionsLayoutModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBranchPositionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatCommandModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatInputConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatReasoningModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallsModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewTextModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatSuggestionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatThreadParentModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatThreadSummaryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatToolbarActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedRejection
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
import uniffi.truapi.ChatActionHandler
import uniffi.truapi.ChatActionLayout
import uniffi.truapi.ChatActions
import uniffi.truapi.ChatActivity
import uniffi.truapi.ChatBotRegistrationStatus
import uniffi.truapi.ChatBranchPosition
import uniffi.truapi.ChatCommandSpec
import uniffi.truapi.ChatCustomMessage
import uniffi.truapi.ChatFile
import uniffi.truapi.ChatInputConfig
import uniffi.truapi.ChatMedia
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatMessageMeta
import uniffi.truapi.ChatReasoning
import uniffi.truapi.ChatResource
import uniffi.truapi.ChatRichText
import uniffi.truapi.ChatRoomBadge
import uniffi.truapi.ChatRoomCall
import uniffi.truapi.ChatRoomCalls
import uniffi.truapi.ChatRoomFeatures
import uniffi.truapi.ChatRoomFooter
import uniffi.truapi.ChatRoomMetadata
import uniffi.truapi.ChatRoomOrder
import uniffi.truapi.ChatRoomParticipation
import uniffi.truapi.ChatRoomPreview
import uniffi.truapi.ChatRoomPreviewText
import uniffi.truapi.ChatRoomRegistrationStatus
import uniffi.truapi.ChatRoomUserState
import uniffi.truapi.ChatSuggestion
import uniffi.truapi.ChatThreadParent
import uniffi.truapi.ChatThreadSummary
import uniffi.truapi.ChatToolbarAction
import uniffi.truapi.HostRejection
import uniffi.truapi.NativeChatFeedRejection

private const val MESSAGING_NOT_SUPPORTED_CODE = "messaging_not_supported"
private const val ROOM = "jollity"

class ProductChatHostBridgeTest {
    private val productId = ProductId.fromStoredValue("dim2.dot")
    private val storedState = mockk<ProductChatStoredState>()

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
        val footed = runCatching { bridge.setRoomFooter("", ChatRoomFooter.Empty) }.exceptionOrNull()

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
    fun `registerBot keeps the persona in the stored state of the product`() = runTest {
        coEvery { storedState.registerBot(productId, "helper", "Helper", "") } returns ChatBotRegistrationStatus.NEW

        assertEquals(ChatBotRegistrationStatus.NEW, ProductChatHostBridge(productId, FakeChatMessaging(), storedState).registerBot("helper", "Helper", ""))
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

        bridge.setRoomFooter(ROOM, ChatRoomFooter.Empty)
        bridge.setRoomFooter(ROOM, ChatRoomFooter.Input(ChatInputConfig(null, true, true, false, null)))

        assertEquals(
            listOf(
                ProductChatIdParameter(ROOM) to ChatRoomFooterModel.Empty,
                ProductChatIdParameter(ROOM) to ChatRoomFooterModel.Input(ChatInputConfigModel.HOST_DEFAULT),
            ),
            api.footers,
        )
    }

    @Test
    fun `a footer that the product draws is kept for the room`() = runTest {
        val api = FakeChatMessaging()

        bridge(api).setRoomFooter(ROOM, ChatRoomFooter.Custom)

        assertEquals(listOf(ProductChatIdParameter(ROOM) to ChatRoomFooterModel.Custom), api.footers)
    }

    @Test
    fun `rich text keeps its v1 media as addresses`() = runTest {
        val sent = withSentMessage(
            ChatMessageContent.RichText(ChatRichText("Look", listOf(ChatMedia("data:image/png;base64,AA==")))),
        )

        assertEquals(
            ProductBotMessage.Body(
                ProductMessageBody.RichText(
                    "Look",
                    listOf(ChatMediaItemModel.Image(ChatResourceModel.Url("data:image/png;base64,AA=="), meta("image/png"))),
                ),
            ),
            sent,
        )
    }

    @Test
    fun `actions become host-drawn buttons`() = runTest {
        val sent = withSentMessage(
            ChatMessageContent.Actions(ChatActions("Pick", listOf(ChatAction("yes", "Yes")), ChatActionLayout.GRID)),
        )

        assertEquals(
            ProductBotMessage.Body(
                ProductMessageBody.Actions("Pick", listOf(ChatActionButtonModel("yes", "Yes")), ChatActionsLayoutModel.GRID),
            ),
            sent,
        )
    }

    @Test
    fun `a file becomes rich text with one file item`() = runTest {
        val sent = withSentMessage(
            ChatMessageContent.File(ChatFile("https://example.com/a.pdf", "a.pdf", "application/pdf", 12uL, "Here")),
        )

        assertEquals(
            ProductBotMessage.Body(
                ProductMessageBody.RichText(
                    "Here",
                    listOf(
                        ChatMediaItemModel.File(
                            ChatResourceModel.Url("https://example.com/a.pdf"),
                            meta("application/pdf").copy(sizeBytes = 12),
                            "a.pdf",
                        ),
                    ),
                ),
            ),
            sent,
        )
    }

    @Test
    fun `a room without declarations answers the host default config`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        coEvery { messaging.roomDeclarations(ProductChatIdParameter(ROOM)) } returns Result.success(null)

        assertEquals(HOST_DEFAULT_CHAT_ROOM_CONFIG, ProductChatHostBridge(productId, messaging, storedState).roomConfig(ROOM))
    }

    @Test
    fun `a declared config is answered as declared`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        val declared = ChatRoomConfigModel.HOST_DEFAULT.copy(productSource = true, allowedReactions = listOf("👍"))
        coEvery { messaging.roomDeclarations(ProductChatIdParameter(ROOM)) } returns
            Result.success(ChatRoomDeclarations.EMPTY.copy(config = declared))

        val config = ProductChatHostBridge(productId, messaging, storedState).roomConfig(ROOM)

        assertTrue(config.productSource)
        assertEquals(listOf("👍"), config.allowedReactions)
    }

    @Test
    fun `each declaration reaches the chat binding as a model`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        val room = ProductChatIdParameter(ROOM)
        coEvery { messaging.setRoomConfig(any(), any()) } returns Result.success(Unit)
        coEvery { messaging.setRoomMetadata(any(), any(), any()) } returns Result.success(Unit)
        coEvery { messaging.setRoomPreview(any(), any()) } returns Result.success(Unit)
        coEvery { messaging.setRoomToolbar(any(), any()) } returns Result.success(Unit)
        coEvery { messaging.setRoomSuggestions(any(), any()) } returns Result.success(Unit)
        coEvery { messaging.setRoomCommands(any(), any()) } returns Result.success(Unit)
        coEvery { messaging.setRoomActivity(any(), any()) } returns Result.success(Unit)
        coEvery { messaging.deleteRoom(any()) } returns Result.success(Unit)
        val bridge = ProductChatHostBridge(productId, messaging, storedState)
        val avatar = byteArrayOf(1, 2)

        bridge.setRoomConfig(ROOM, HOST_DEFAULT_CHAT_ROOM_CONFIG.copy(reportSeen = true))
        bridge.setRoomMetadata(ROOM, ChatRoomMetadata("Lobby", ChatResource.Inline("image/png", avatar), "Play here"))
        bridge.setRoomPreview(ROOM, ChatRoomPreview(ChatRoomOrder.PIN_TO_TOP, 5uL, ChatRoomBadge.UnreadCount(2u), ChatRoomPreviewText("typing", true)))
        bridge.setRoomToolbar(ROOM, listOf(ChatToolbarAction("help", "Help", ChatResource.Preimage(key()), ChatActionHandler.Callback)))
        bridge.setRoomSuggestions(ROOM, listOf(ChatSuggestion("s", "Hi", true)))
        bridge.setRoomCommands(ROOM, listOf(ChatCommandSpec("roll", "Roll", null)))
        bridge.setRoomActivity(ROOM, ChatActivity("Bot is typing", null))
        bridge.setRoomActivity(ROOM, null)
        bridge.deleteRoom(ROOM)

        coVerify { messaging.setRoomConfig(room, ChatRoomConfigModel.HOST_DEFAULT.copy(reportSeen = true)) }
        coVerify {
            messaging.setRoomMetadata(
                room,
                "Lobby",
                ChatRoomMetadataModel("Play here", ChatResourceModel.Inline("image/png", avatar.toDataByteArray())),
            )
        }
        coVerify {
            messaging.setRoomPreview(
                room,
                ChatRoomPreviewModel(Order.PinToTop, 5, ChatRoomBadgeModel.Count(2), ChatRoomPreviewTextModel("typing", true)),
            )
        }
        coVerify {
            messaging.setRoomToolbar(
                room,
                listOf(ChatToolbarActionModel("help", "Help", ChatResourceModel.Preimage(key().toDataByteArray()), ChatActionHandlerModel.Callback)),
            )
        }
        coVerify { messaging.setRoomSuggestions(room, listOf(ChatSuggestionModel("s", "Hi", true))) }
        coVerify { messaging.setRoomCommands(room, listOf(ChatCommandModel("roll", "Roll", null))) }
        coVerify { messaging.setRoomActivity(room, ChatRoomActivityModel("Bot is typing", null)) }
        coVerify { messaging.setRoomActivity(room, null) }
        coVerify { messaging.deleteRoom(room) }
    }

    @Test
    fun `a declaration for an unknown room is rejected`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        coEvery { messaging.setRoomPreview(any(), any()) } returns Result.failure(UnknownChatRoomException())

        val thrown = runCatching {
            ProductChatHostBridge(productId, messaging, storedState).setRoomPreview(ROOM, ChatRoomPreview(ChatRoomOrder.TIMESTAMP, null, null, null))
        }.exceptionOrNull()

        assertTrue(thrown is HostRejection)
        assertTrue(thrown!!.message!!.contains("no such chat room"))
    }

    @Test
    fun `capabilities lists only the features that this build serves from end to end`() = runTest {
        assertEquals(
            listOf(
                "product_source_rooms",
                "reaction_details",
                "participants",
                "message_menus",
                "participant_menus",
                "toolbar",
                "suggestions",
                "commands",
                "activity",
                "custom_bodies",
                "custom_footer",
                "inline_attachments",
                "search",
                "bot_personas",
                "mute_archive",
                "threads",
                "branches",
                "reasoning_rows",
                "read_aloud",
                "call_buttons",
            ),
            bridge(FakeChatMessaging()).capabilities(),
        )
    }

    @Test
    fun `a room without declared features answers the host default features`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        coEvery { messaging.roomDeclarations(ProductChatIdParameter(ROOM)) } returns Result.success(ChatRoomDeclarations.EMPTY)

        assertEquals(HOST_DEFAULT_CHAT_ROOM_FEATURES, ProductChatHostBridge(productId, messaging, storedState).roomFeatures(ROOM))
    }

    @Test
    fun `declared features are answered as declared, also those that this host does not serve`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        val declared = ChatRoomFeaturesModel(
            preimageAttachments = true,
            search = false,
            calls = ChatRoomCallsModel(audio = true, video = false),
            threadOf = ChatThreadParentModel("lobby", "m1"),
            readAloud = true,
        )
        coEvery { messaging.roomDeclarations(ProductChatIdParameter(ROOM)) } returns
            Result.success(ChatRoomDeclarations.EMPTY.copy(features = declared))

        val features = ProductChatHostBridge(productId, messaging, storedState).roomFeatures(ROOM)

        assertEquals(ChatRoomFeatures(true, false, ChatRoomCalls(true, false), ChatThreadParent("lobby", "m1"), true, false), features)
    }

    @Test
    fun `features, message meta and the call reach the chat binding as models`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        val room = ProductChatIdParameter(ROOM)
        coEvery { messaging.setRoomFeatures(any(), any()) } returns Result.success(Unit)
        coEvery { messaging.setMessageMeta(any(), any(), any()) } returns Result.success(Unit)
        coEvery { messaging.setRoomCall(any(), any()) } returns Result.success(Unit)
        val bridge = ProductChatHostBridge(productId, messaging, storedState)
        val meta = ChatMessageMeta(
            thread = ChatThreadSummary("thread-1", 4_000_000_000u, 9uL, listOf("p1")),
            branch = ChatBranchPosition(2u.toUShort(), 60_000u.toUShort()),
            reasoning = ChatReasoning(4_000u),
        )

        bridge.setRoomFeatures(ROOM, HOST_DEFAULT_CHAT_ROOM_FEATURES.copy(search = true))
        bridge.setMessageMeta(ROOM, "m1", meta)
        bridge.setRoomCall(ROOM, ChatRoomCall.Ringing("c1", video = true, caller = "p1"))
        bridge.setRoomCall(ROOM, null)

        coVerify { messaging.setRoomFeatures(room, ChatRoomFeaturesModel.HOST_DEFAULT.copy(search = true)) }
        coVerify {
            messaging.setMessageMeta(
                room,
                "m1",
                ChatMessageMetaModel(
                    thread = ChatThreadSummaryModel("thread-1", 4_000_000_000, 9, listOf("p1")),
                    branch = ChatBranchPositionModel(2, 60_000),
                    reasoning = ChatReasoningModel(4_000),
                ),
            )
        }
        coVerify { messaging.setRoomCall(room, ChatRoomCallModel.Ringing("c1", video = true, caller = "p1")) }
        coVerify { messaging.setRoomCall(room, null) }
    }

    @Test
    fun `meta for a message that the room does not hold is refused as an unknown message`() = runTest {
        val messaging = mockk<ProductChatMessaging>()
        coEvery { messaging.setMessageMeta(any(), any(), any()) } returns Result.failure(ProductFeedRejection.UnknownMessage())

        val thrown = runCatching {
            ProductChatHostBridge(productId, messaging, storedState).setMessageMeta(ROOM, "missing", ChatMessageMeta(null, null, null))
        }.exceptionOrNull()

        assertTrue("expected UnknownMessage but was $thrown", thrown is NativeChatFeedRejection.UnknownMessage)
    }

    @Test
    fun `the user state of the rooms and the bot ids come from the stored state, without a chat binding`() = runTest {
        val states = listOf(ChatRoomUserState(ROOM, ULong.MAX_VALUE, archived = true))
        coEvery { storedState.roomUserStates(productId) } returns states
        coEvery { storedState.botIds(productId) } returns listOf("helper")
        val bridge = ProductChatHostBridge(productId, mockk(), storedState)

        assertEquals(states, bridge.roomUserStates())
        assertEquals(listOf("helper"), bridge.bots())
    }

    private suspend fun withSentMessage(content: ChatMessageContent): ProductBotMessage? {
        var sent: ProductBotMessage? = null
        val api = FakeChatMessaging(onSendMessage = { _, message -> sent = message; Result.success("message-id") })
        bridge(api).postMessage(ROOM, content, alt = null)
        return sent
    }

    private fun meta(mimeType: String) = ChatMediaMetaModel(mimeType, null, null, null, null, null)

    private fun key() = ByteArray(32) { 5 }

    private fun bridge(api: FakeChatMessaging) = ProductChatHostBridge(productId, api, storedState)
}

private fun <T> messagingNotSupported(): Result<T> = Result.failure(
    HostCallException(MESSAGING_NOT_SUPPORTED_CODE, "Product messaging is not supported in this context"),
)
