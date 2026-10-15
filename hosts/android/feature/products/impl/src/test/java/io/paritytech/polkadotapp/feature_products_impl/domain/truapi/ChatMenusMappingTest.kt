package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionHandlerModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatBuiltInMessageActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatProductActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductReactionSummaryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageContent
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ChatActionHandler
import uniffi.truapi.ChatBuiltInMessageAction
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageMenu
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatParticipantMenu
import uniffi.truapi.ChatProductAction
import uniffi.truapi.ChatReactionSummary
import uniffi.truapi.ChatResource

class ChatMenusMappingTest {
    @Test
    fun `a menu of the product keeps its entries and the hidden built-ins`() {
        val menu = ChatMessageMenu.ExtendBuiltIn(
            actions = listOf(
                ChatProductAction("pin", "Pin", ChatResource.Inline("image/png", byteArrayOf(1)), destructive = false, ChatActionHandler.Callback),
                ChatProductAction("docs", "Docs", icon = null, destructive = true, ChatActionHandler.Deeplink("polkadot://weather.dot/docs")),
            ),
            hidden = listOf(ChatBuiltInMessageAction.EDIT, ChatBuiltInMessageAction.REACT),
        )

        assertEquals(
            ChatMessageMenuModel.ExtendBuiltIn(
                actions = listOf(
                    ChatProductActionModel(
                        "pin",
                        "Pin",
                        ChatResourceModel.Inline("image/png", byteArrayOf(1).toDataByteArray()),
                        destructive = false,
                        ChatActionHandlerModel.Callback,
                    ),
                    ChatProductActionModel("docs", "Docs", icon = null, destructive = true, ChatActionHandlerModel.Deeplink("polkadot://weather.dot/docs")),
                ),
                hidden = setOf(ChatBuiltInMessageActionModel.EDIT, ChatBuiltInMessageActionModel.REACT),
            ),
            menu.toModel(),
        )
    }

    @Test
    fun `a participant menu of the product keeps its entries`() {
        val action = ChatProductAction("mute", "Mute", icon = null, destructive = false, ChatActionHandler.Callback)
        val model = ChatProductActionModel("mute", "Mute", icon = null, destructive = false, ChatActionHandlerModel.Callback)

        assertEquals(ChatParticipantMenuModel.Custom(listOf(model)), ChatParticipantMenu.Custom(listOf(action)).toModel())
        assertEquals(ChatParticipantMenuModel.ExtendBuiltIn(listOf(model)), ChatParticipantMenu.ExtendBuiltIn(listOf(action)).toModel())
    }

    @Test
    fun `the query message carries no media and no custom payload`() {
        val media = ChatMediaItemModel.Image(
            ChatResourceModel.Inline("image/png", byteArrayOf(1, 2).toDataByteArray()),
            ChatMediaMetaModel("image/png", sizeBytes = 2, width = 1, height = 1, placeholder = null, alt = null),
        )
        val richText = message(ChatMessage.Content.ProductBody(ProductMessageBody.RichText("look", listOf(media))))
        val custom = message(
            ChatMessage.Content.Custom(
                rendererId = "ProductBot_weather.dot",
                content = Result.success(ProductsMessageContent("card", byteArrayOf(9, 9).toDataByteArray(), "A card")),
            ),
        )

        assertEquals(ChatMessageBody.RichText("look", emptyList()), richText.toQueryMessage().body)
        val customBody = custom.toQueryMessage().body as ChatMessageBody.Custom
        assertEquals("card", customBody.messageType)
        assertArrayEquals(ByteArray(0), customBody.payload)
        assertEquals("A card", customBody.previewText)
    }

    @Test
    fun `the query message keeps the ids, the status and the reactions`() {
        val query = message(ChatMessage.Content.Text("hi")).toQueryMessage()

        assertEquals("m1", query.messageId)
        assertEquals("alice", query.origin.participantId)
        assertEquals(5uL, query.timestamp)
        assertEquals(ChatMessageStatus.DELIVERED, query.status)
        assertEquals("m0", query.replyTo)
        assertEquals(listOf(ChatReactionSummary("👍", 2u, reactedByUser = true)), query.reactions)
    }

    private fun message(content: ChatMessage.Content) = ProductRoomMessage(
        messageId = "m1",
        participantId = "alice",
        timestamp = 5L,
        content = content,
        status = ProductMessageStatusModel.DELIVERED,
        replyTo = "m0",
        editedAt = null,
        reactions = listOf(ProductReactionSummaryModel("👍", count = 2, reactedByUser = true)),
    )
}
