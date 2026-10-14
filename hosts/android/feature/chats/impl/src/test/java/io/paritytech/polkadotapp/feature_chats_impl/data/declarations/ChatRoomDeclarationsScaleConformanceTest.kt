package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.database.model.ChatRoomDeclarationsLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionHandlerModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatCommandModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatInputConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatInputPaymentModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewTextModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatSuggestionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatToolbarActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatResourceLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.toDomain
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Backward compatibility of the SCALE values in `chat_room_declarations`. The config and the preview also
 * match the bytes of the Chat Extension v2 wire types.
 *
 * DO NOT modify the encoded hex values below: they are the frozen format that must always decode.
 */
class ChatRoomDeclarationsScaleConformanceTest {
    @Test
    fun `decode a declared config from encoded bytes`() {
        assertEquals(
            ChatRoomConfigModel(
                productSource = true,
                reactionDetails = false,
                participants = true,
                messageActions = false,
                participantActions = true,
                allowedReactions = listOf("👍", "🔥"),
                placeholder = "Ask me",
                reportSeen = true,
            ),
            decode<ChatRoomConfigLocal>(CONFIG).toDomain(),
        )
        assertEquals(ChatRoomConfigModel.HOST_DEFAULT, decode<ChatRoomConfigLocal>(CONFIG_DEFAULT).toDomain())
    }

    @Test
    fun `decode previews with each badge from encoded bytes`() {
        assertEquals(
            ChatRoomPreviewModel(
                order = Order.PinToTop,
                timestamp = 1_700_000_000_000,
                badge = ChatRoomBadgeModel.Count(3),
                text = ChatRoomPreviewTextModel("Bot is typing", ongoing = true),
            ),
            decode<ChatRoomPreviewLocal>(PREVIEW).toDomain(),
        )
        assertEquals(
            ChatRoomPreviewModel(Order.ByTimestamp, null, ChatRoomBadgeModel.Image(ChatResourceModel.Preimage(key(7))), null),
            decode<ChatRoomPreviewLocal>(PREVIEW_IMAGE).toDomain(),
        )
        assertEquals(ChatRoomBadgeModel.Hidden, decode<ChatRoomPreviewLocal>(PREVIEW_HIDDEN).toDomain().badge)
    }

    @Test
    fun `decode toolbar, suggestions and commands from encoded bytes`() {
        assertEquals(
            listOf(
                ChatToolbarActionModel(
                    "help",
                    "Help",
                    ChatResourceModel.Inline("image/png", byteArrayOf(9).toDataByteArray()),
                    ChatActionHandlerModel.Deeplink("polkadot://a"),
                ),
                ChatToolbarActionModel("reset", "Reset", ChatResourceModel.Preimage(key(1)), ChatActionHandlerModel.Callback),
            ),
            decode<List<ChatToolbarActionLocal>>(TOOLBAR).map { it.toDomain() },
        )
        assertEquals(
            listOf(ChatSuggestionModel("s1", "Hi", sendsText = true), ChatSuggestionModel("s2", "Menu", sendsText = false)),
            decode<List<ChatSuggestionLocal>>(SUGGESTIONS).map { it.toDomain() },
        )
        assertEquals(
            listOf(ChatCommandModel("roll", "Roll a die", "sides"), ChatCommandModel("help", "Show help", null)),
            decode<List<ChatCommandLocal>>(COMMANDS).map { it.toDomain() },
        )
    }

    @Test
    fun `decode each footer and an avatar from encoded bytes`() {
        assertEquals(
            ChatRoomFooterModel.Input(
                ChatInputConfigModel("Say", photo = true, video = false, file = true, payment = ChatInputPaymentModel.Enabled(key(2))),
            ),
            decode<ChatRoomFooterLocal>(FOOTER_INPUT).toDomain(),
        )
        assertEquals(ChatRoomFooterModel.Empty, decode<ChatRoomFooterLocal>(FOOTER_EMPTY).toDomain())
        assertEquals(ChatRoomFooterModel.Custom, decode<ChatRoomFooterLocal>(FOOTER_CUSTOM).toDomain())
        assertEquals(
            ChatResourceModel.Inline("image/webp", byteArrayOf(1, 2).toDataByteArray()),
            decode<ChatResourceLocal>(AVATAR).toDomain(),
        )
    }

    @Test
    fun `a stored row reads back the declarations that were encoded into it`() {
        val declarations = ChatRoomDeclarations(
            config = ChatRoomConfigModel.HOST_DEFAULT.copy(reportSeen = true),
            configGeneration = 4,
            metadata = ChatRoomMetadataModel("About", ChatResourceModel.Preimage(key(3))),
            preview = ChatRoomPreviewModel(Order.ByTimestamp, null, ChatRoomBadgeModel.Hidden, null),
            toolbar = listOf(ChatToolbarActionModel("a", "A", ChatResourceModel.Preimage(key(4)), ChatActionHandlerModel.Callback)),
            suggestions = emptyList(),
            commands = listOf(ChatCommandModel("go", "Go", null)),
            footer = ChatRoomFooterModel.Empty,
        )

        val row = ChatRoomDeclarationsLocal(
            chatId = byteArrayOf(1),
            config = declarations.config!!.encode(),
            configGeneration = 4,
            description = "About",
            avatar = declarations.metadata.encodeAvatar(),
            preview = declarations.preview!!.encode(),
            toolbar = declarations.toolbar!!.encodeToolbar(),
            suggestions = declarations.suggestions!!.encodeSuggestions(),
            commands = declarations.commands!!.encodeCommands(),
            footer = declarations.footer!!.encode(),
        )

        assertEquals(declarations, row.toDomain())
    }

    @Test
    fun `a part that does not decode reads as not declared`() {
        val row = ChatRoomDeclarationsLocal(byteArrayOf(1), byteArrayOf(9), 1, null, null, null, null, null, null, byteArrayOf(7))

        val declarations = row.toDomain()

        assertNull(declarations.config)
        assertNull(declarations.footer)
    }

    private inline fun <reified T> decode(hex: String): T = BinaryScale.decodeFromByteArray<T>(hex.fromHex())

    private fun key(fill: Int) = ByteArray(32) { fill.toByte() }.toDataByteArray()

    private companion object {
        const val CONFIG = "0100010001010810f09f918d10f09f94a5011841736b206d6501"
        const val CONFIG_DEFAULT = "0000000000000000"
        const val PREVIEW = "01010068e5cf8b0100000101030000000134426f7420697320747970696e6701"
        const val PREVIEW_IMAGE = "000001020180070707070707070707070707070707070707070707070707070707070707070700"
        const val PREVIEW_HIDDEN = "0000010000"
        const val TOOLBAR = "081068656c701048656c700024696d6167652f706e6704090030706f6c6b61646f743a2f2f61147265736574145265736574018001" +
            "0101010101010101010101010101010101010101010101010101010101010101"
        const val SUGGESTIONS = "0808733108486901087332104d656e7500"
        const val COMMANDS = "0810726f6c6c28526f6c6c206120646965011473696465731068656c702453686f772068656c7000"
        const val FOOTER_INPUT = "00010c5361790100010100800202020202020202020202020202020202020202020202020202020202020202"
        const val FOOTER_EMPTY = "01"
        const val FOOTER_CUSTOM = "02"
        const val AVATAR = "0028696d6167652f77656270080102"
    }
}
