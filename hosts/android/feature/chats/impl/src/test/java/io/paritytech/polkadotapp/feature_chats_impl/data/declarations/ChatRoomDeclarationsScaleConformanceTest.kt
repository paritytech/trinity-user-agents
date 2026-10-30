package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.database.model.ChatRoomDeclarationsLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRendererId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatPreviewReactionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.AlwaysFailCustomContentDecoder
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.CustomContentDecoder
import kotlinx.serialization.encodeToByteArray
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Backward compatibility of the SCALE values in `chat_room_declarations`. The config keeps its first stored layout, and
 * a preview in its first layout still decodes.
 *
 * DO NOT modify the encoded hex values below: they are the frozen format that must always decode.
 */
class ChatRoomDeclarationsScaleConformanceTest {
    private val noCustomContent = AlwaysFailCustomContentDecoder()

    @Test
    fun `decode a declared config from encoded bytes`() {
        assertEquals(
            ChatRoomConfigModel(productSource = true, reactionDetails = false, reportSeen = true),
            decode<ChatRoomConfigLocal>(CONFIG).toDomain(),
        )
        assertEquals(ChatRoomConfigModel.HOST_DEFAULT, decode<ChatRoomConfigLocal>(CONFIG_DEFAULT).toDomain())
    }

    // A preview stored before the pinning and the activity keeps its row, unpinned and without an activity.
    @Test
    fun `decode previews in the first layout with each badge from encoded bytes`() {
        assertEquals(
            ChatRoomPreviewModel(
                timestamp = 1_700_000_000_000,
                badge = ChatRoomBadgeModel.Count(3),
                lastMessage = null,
                lastReaction = ChatPreviewReactionModel(emoji = "👍", byUser = false, message = null, removed = false),
                unseenReactions = 2,
                pinned = false,
                activity = null,
            ),
            storedPreview(PREVIEW),
        )
        assertEquals(ChatRoomBadgeModel.Count(0), storedPreview(PREVIEW_HIDDEN)?.badge)
    }

    @Test
    fun `a pinned preview with an image badge and an activity encodes to the frozen bytes and decodes back`() {
        val preview = ChatRoomPreviewModel(
            timestamp = null,
            badge = ChatRoomBadgeModel.Image("https://example.invalid/badge.png"),
            lastMessage = null,
            lastReaction = null,
            unseenReactions = 0,
            pinned = true,
            activity = ChatRoomActivityModel("Ben is typing", ongoing = true),
        )

        assertEquals(PREVIEW_PINNED, preview.encode(noCustomContent).toHexString())
        assertEquals(preview, storedPreview(PREVIEW_PINNED))
    }

    @Test
    fun `a stored row reads back the declarations that were encoded into it`() {
        val lastMessage = ProductRoomMessage(
            messageId = "m1",
            participantId = "product",
            timestamp = 5,
            // A content with equality, so the whole declarations compare.
            content = ChatMessage.Content.ContactAdded,
            status = ProductMessageStatusModel.READ,
            replyTo = null,
            editedAt = 6,
            reactions = emptyList(),
            request = true,
        )
        val declarations = ChatRoomDeclarations(
            config = ChatRoomConfigModel.HOST_DEFAULT.copy(reportSeen = true),
            configGeneration = 4,
            metadata = ChatRoomMetadataModel("About", placeholder = "Nothing here yet"),
            preview = ChatRoomPreviewModel(
                timestamp = 9,
                badge = ChatRoomBadgeModel.Count(1),
                lastMessage = lastMessage,
                lastReaction = ChatPreviewReactionModel("🔥", byUser = true, message = lastMessage, removed = true),
                unseenReactions = 0,
                pinned = true,
                activity = ChatRoomActivityModel("Ben is typing", ongoing = false),
            ),
        )

        val row = ChatRoomDeclarationsLocal(
            chatId = byteArrayOf(1),
            config = declarations.config!!.encode(),
            configGeneration = 4,
            description = "About",
            placeholder = "Nothing here yet",
            preview = declarations.preview!!.encode(noCustomContent),
            features = null,
            contact = null,
        )

        assertEquals(declarations, row.toDomain(noCustomContent))
    }

    @Test
    fun `a preview message keeps its text`() {
        val message = ProductRoomMessage("m1", "product", 5, ChatMessage.Content.Text("hi"), ProductMessageStatusModel.SENT, null, null, emptyList(), false)
        val preview = previewOf(message)

        val decoded = decode<ChatRoomPreviewLocal>(preview.encode(noCustomContent).toHexString()).toDomain(noCustomContent)

        assertEquals("hi", (decoded.lastMessage!!.content as ChatMessage.Content.Text).text)
    }

    // The renderer of the product encodes and decodes a custom message, so the chat list keeps the content that the
    // room screen draws.
    @Test
    fun `a custom preview message keeps the content of its renderer`() {
        val content = ChatMessage.Content.Custom(rendererId = "game.dot", content = Result.success("poll"))
        val message = ProductRoomMessage("m1", "product", 5, content, ProductMessageStatusModel.SENT, null, null, emptyList(), false)
        val preview = previewOf(message)

        val decoded = decode<ChatRoomPreviewLocal>(preview.encode(TextContent).toHexString()).toDomain(TextContent)

        assertEquals(content, decoded.lastMessage!!.content)
    }

    @Test
    fun `a part that does not decode reads as not declared`() {
        val row = ChatRoomDeclarationsLocal(byteArrayOf(1), byteArrayOf(9), 1, null, null, byteArrayOf(1, 9), null, null)

        val declarations = row.toDomain(noCustomContent)

        assertNull(declarations.config)
        assertNull(declarations.preview)
    }

    @Test
    fun `the config keeps writing its first layout`() {
        assertEquals(CONFIG_DEFAULT, BinaryScale.encodeToByteArray(ChatRoomConfigModel.HOST_DEFAULT.toLocal()).toHexString())
    }

    private inline fun <reified T> decode(hex: String): T = BinaryScale.decodeFromByteArray<T>(hex.fromHex())

    private fun storedPreview(hex: String): ChatRoomPreviewModel? =
        ChatRoomDeclarationsLocal(byteArrayOf(1), null, 0, null, null, hex.fromHex(), null, null).toDomain(noCustomContent).preview

    private fun previewOf(message: ProductRoomMessage) = ChatRoomPreviewModel(
        timestamp = null,
        badge = null,
        lastMessage = message,
        lastReaction = null,
        unseenReactions = 0,
        pinned = false,
        activity = null,
    )

    private object TextContent : CustomContentDecoder {
        override fun decode(rendererId: CustomChatMessageRendererId, content: ByteArray): Result<Any?> =
            Result.success(content.decodeToString())

        override fun encode(rendererId: CustomChatMessageRendererId, value: Any?): Result<ByteArray> =
            Result.success((value as String).encodeToByteArray())
    }

    private companion object {
        const val CONFIG = "01000100010810f09f918d10f09f94a5011841736b206d6501"
        const val CONFIG_DEFAULT = "00000000000000"
        const val PREVIEW = "010068e5cf8b010000010103000000000110f09f918d00000002000000"
        const val PREVIEW_HIDDEN = "000100000000000000"
        const val PREVIEW_PINNED = "0001028468747470733a2f2f6578616d706c652e696e76616c69642f62616467652e706e6700000000000001013442656e20697320747970696e6701"
    }
}
