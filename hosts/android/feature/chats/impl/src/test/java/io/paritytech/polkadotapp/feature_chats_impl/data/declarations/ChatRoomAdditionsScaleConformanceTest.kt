package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBranchPositionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatReasoningModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallsModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatThreadParentModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatThreadSummaryModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Backward compatibility of the stored values of the Chat Extension v2 additions: `chat_room_declarations.features`
 * and `chat_message_meta.meta`. Each value is the version byte 1, then the SCALE bytes of the wire type.
 *
 * DO NOT modify the encoded hex values below: they are the frozen format that must always decode.
 */
class ChatRoomAdditionsScaleConformanceTest {
    @Test
    fun `decode declared features from encoded bytes`() {
        assertEquals(FEATURES_DECLARED_MODEL, FEATURES_DECLARED.fromHex().decodeFeatures())
        assertEquals(ChatRoomFeaturesModel.HOST_DEFAULT, FEATURES_DEFAULT.fromHex().decodeFeatures())
    }

    @Test
    fun `features encode to the frozen bytes`() {
        assertEquals(FEATURES_DECLARED, FEATURES_DECLARED_MODEL.encode().toHexString())
        assertEquals(FEATURES_DEFAULT, ChatRoomFeaturesModel.HOST_DEFAULT.encode().toHexString())
    }

    @Test
    fun `decode message meta from encoded bytes`() {
        assertEquals(META_FULL_MODEL, META_FULL.fromHex().decodeMessageMeta())
        assertEquals(META_WIDE_MODEL, META_WIDE.fromHex().decodeMessageMeta())
        assertEquals(ChatMessageMetaModel(null, null, null), META_EMPTY.fromHex().decodeMessageMeta())
    }

    @Test
    fun `message meta encodes to the frozen bytes, with the full range of the unsigned wire integers`() {
        assertEquals(META_FULL, META_FULL_MODEL.encode().toHexString())
        assertEquals(META_WIDE, META_WIDE_MODEL.encode().toHexString())
    }

    @Test
    fun `a value of an unknown version or that does not decode reads as not declared`() {
        assertNull(("02" + FEATURES_DEFAULT.drop(2)).fromHex().decodeFeatures())
        assertNull("0101".fromHex().decodeFeatures())
        assertNull(ByteArray(0).decodeMessageMeta())
    }

    private companion object {
        const val FEATURES_DECLARED = "01010001010001146c6f626279086d3101"
        const val FEATURES_DEFAULT = "010000000000"
        const val META_FULL = "010108743103000000010068e5cf8b0100000808703128626f743a68656c70657201020003000101a00f0000"
        const val META_WIDE = "0101047400286bee000001010060ea0100"
        const val META_EMPTY = "01000000"

        val FEATURES_DECLARED_MODEL = ChatRoomFeaturesModel(
            preimageAttachments = true,
            search = false,
            calls = ChatRoomCallsModel(audio = true, video = false),
            threadOf = ChatThreadParentModel(roomId = "lobby", messageId = "m1"),
            readAloud = true,
        )

        val META_FULL_MODEL = ChatMessageMetaModel(
            thread = ChatThreadSummaryModel("t1", replyCount = 3, lastReplyAt = 1_700_000_000_000, participantIds = listOf("p1", "bot:helper")),
            branch = ChatBranchPositionModel(index = 2, count = 3),
            reasoning = ChatReasoningModel(durationMs = 4_000),
        )

        val META_WIDE_MODEL = ChatMessageMetaModel(
            thread = ChatThreadSummaryModel("t", replyCount = 4_000_000_000, lastReplyAt = null, participantIds = emptyList()),
            branch = ChatBranchPositionModel(index = 1, count = 60_000),
            reasoning = ChatReasoningModel(durationMs = null),
        )
    }
}
