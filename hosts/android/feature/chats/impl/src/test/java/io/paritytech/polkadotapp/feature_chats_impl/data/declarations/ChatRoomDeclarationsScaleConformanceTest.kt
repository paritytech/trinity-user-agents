package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.database.model.ChatRoomDeclarationsLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatPreviewReactionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import kotlinx.serialization.encodeToByteArray
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Backward compatibility of the SCALE values in `chat_room_declarations`. The config keeps its first stored layout.
 *
 * DO NOT modify the encoded hex values below: they are the frozen format that must always decode.
 */
class ChatRoomDeclarationsScaleConformanceTest {
    @Test
    fun `decode a declared config from encoded bytes`() {
        assertEquals(
            ChatRoomConfigModel(productSource = true, reactionDetails = false, reportSeen = true),
            decode<ChatRoomConfigLocal>(CONFIG).toDomain(),
        )
        assertEquals(ChatRoomConfigModel.HOST_DEFAULT, decode<ChatRoomConfigLocal>(CONFIG_DEFAULT).toDomain())
    }

    @Test
    fun `decode previews with each badge from encoded bytes`() {
        assertEquals(
            ChatRoomPreviewModel(
                timestamp = 1_700_000_000_000,
                badge = ChatRoomBadgeModel.Count(3),
                lastMessage = null,
                lastReaction = ChatPreviewReactionModel(emoji = "👍", byUser = false, message = null, removed = false),
                unseenReactions = 2,
            ),
            decode<ChatRoomPreviewLocal>(PREVIEW).toDomain(),
        )
        assertEquals(ChatRoomBadgeModel.Hidden, decode<ChatRoomPreviewLocal>(PREVIEW_HIDDEN).toDomain().badge)
    }

    @Test
    fun `a preview encodes to the frozen bytes`() {
        val preview = ChatRoomPreviewModel(
            timestamp = null,
            badge = ChatRoomBadgeModel.Hidden,
            lastMessage = null,
            lastReaction = null,
            unseenReactions = 0,
        )

        assertEquals(PREVIEW_HIDDEN, preview.encode().toHexString())
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
            metadata = ChatRoomMetadataModel("About"),
            preview = ChatRoomPreviewModel(
                timestamp = 9,
                badge = ChatRoomBadgeModel.Count(1),
                lastMessage = lastMessage,
                lastReaction = ChatPreviewReactionModel("🔥", byUser = true, message = lastMessage, removed = true),
                unseenReactions = 0,
            ),
        )

        val row = ChatRoomDeclarationsLocal(
            chatId = byteArrayOf(1),
            config = declarations.config!!.encode(),
            configGeneration = 4,
            description = "About",
            preview = declarations.preview!!.encode(),
            features = null,
            contact = null,
        )

        assertEquals(declarations, row.toDomain())
    }

    @Test
    fun `a preview message keeps its text`() {
        val message = ProductRoomMessage("m1", "product", 5, ChatMessage.Content.Text("hi"), ProductMessageStatusModel.SENT, null, null, emptyList(), false)
        val preview = ChatRoomPreviewModel(timestamp = null, badge = null, lastMessage = message, lastReaction = null, unseenReactions = 0)

        val decoded = decode<ChatRoomPreviewLocal>(preview.encode().toHexString()).toDomain()

        assertEquals("hi", (decoded.lastMessage!!.content as ChatMessage.Content.Text).text)
    }

    @Test
    fun `a part that does not decode reads as not declared`() {
        val row = ChatRoomDeclarationsLocal(byteArrayOf(1), byteArrayOf(9), 1, null, byteArrayOf(1, 9), null, null)

        val declarations = row.toDomain()

        assertNull(declarations.config)
        assertNull(declarations.preview)
    }

    @Test
    fun `the config keeps writing its first layout`() {
        assertEquals(CONFIG_DEFAULT, BinaryScale.encodeToByteArray(ChatRoomConfigModel.HOST_DEFAULT.toLocal()).toHexString())
    }

    private inline fun <reified T> decode(hex: String): T = BinaryScale.decodeFromByteArray<T>(hex.fromHex())

    private companion object {
        const val CONFIG = "01000100010810f09f918d10f09f94a5011841736b206d6501"
        const val CONFIG_DEFAULT = "00000000000000"
        const val PREVIEW = "010068e5cf8b010000010103000000000110f09f918d00000002000000"
        const val PREVIEW_HIDDEN = "000100000000000000"
    }
}
